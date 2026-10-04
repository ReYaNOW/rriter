    use super::*;

    use super::super::key_routes::{route_position, RouteId};

    #[test]
    fn keymap_recording_precedes_database_query_review() {
        assert!(route_position(RouteId::KeymapSettings) < route_position(RouteId::QueryReview), "keymap recording must reject Enter before SQL review commits it");
    }

    #[test]
    fn api_command_route_follows_modal_and_database_and_precedes_bound_commands() {
        assert!(route_position(RouteId::ConfirmModal) < route_position(RouteId::DatabaseTableCommands), "modal owner precedes DatabaseTable dispatch");
        assert!(route_position(RouteId::DatabaseTableCommands) < route_position(RouteId::ApiClient), "DatabaseTable dispatch precedes API Client");
        assert!(route_position(RouteId::ApiClient) < route_position(RouteId::BoundCommands), "API Client precedes generic bound commands");
    }

    #[test]
    fn terminal_close_shortcut_precedes_final_keyboard_dispatch() {
        assert!(route_position(RouteId::TerminalCloseTab) < route_position(RouteId::FinalRoute), "terminal tab close precedes final terminal and editor routing");
    }

    #[test]
    fn api_global_command_does_not_fire_through_main_handler_while_settings_is_open() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            panic!("test app must initialize");
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.show_settings = true;
        app.ide_panel.open(crate::app::PanelId::ApiClient);
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord parses");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::GitToggleGraph, chord);
        app.keymap = crate::keymap::Keymap::build(&overrides);
        assert!(app.active_tab_is_api_client() || app.ide_panel.is_open(crate::app::PanelId::ApiClient), "Settings regression fixture must retain a visible API Client surface: active_api={} open_api={}", app.active_tab_is_api_client(), app.ide_panel.is_open(crate::app::PanelId::ApiClient));
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        let event = KeyInput {
            physical_key: PhysicalKey::Code(KeyCode::KeyG),
            logical_text: None,
            text: None,
            state: ElementState::Pressed,
            repeat: false,
        };
        app.handle_main_key_input(&crate::app::events::host_loop::HostLoop::headless(&state), event);
        assert_eq!(app.ide_panel.git.bottom_pane, crate::app::git_panel::GitBottomPane::Closed, "Settings must own the key before API/global command dispatch: pane={:?}", app.ide_panel.git.bottom_pane);
    }

    fn database_table_hotkey_app() -> (App, crate::keymap::Chord) {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            panic!("test app must initialize");
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        let tab_id = crate::app::database::DatabaseTabId(7);
        let mut tab = crate::app::app_behavior_tests::tab_with("items", None, "");
        let table = crate::app::database::DatabaseTableTabState {
            loading: false,
            metadata: Some(crate::app::database::DatabaseTableMetadata {
            database_name: "test".to_string(),
            table_name: "items".to_string(),
            columns: vec![crate::app::database::DatabaseColumnInfo {
                ordinal: 1,
                name: "value".to_string(),
                type_name: "text".to_string(),
                type_oid: 25,
                type_kind: crate::app::database::DatabaseTypeKind::Other,
                nullable: true,
                default_expression: None,
                identity: false,
                generated: false,
                primary_key: false,
                enum_values: Vec::new(),
            }],
            primary_key_columns: Vec::new(),
            editable: true,
            read_only_reason: None,
            notices: Vec::new(),
            }),
            ..crate::app::database::DatabaseTableTabState::default()
        };
        tab.kind = crate::app::EditorTabKind::DatabaseTable(
            crate::app::database::DatabaseTableTabMeta {
                tab_id,
                connection_id: crate::app::database::DatabaseConnectionId(3),
                database_name: "test".to_string(),
                table_name: "items".to_string(),
            },
            table,
        );
        app.tabs = vec![tab];
        app.active_tab = 0;
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord parses");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::DatabaseTableAddRow, chord);
        app.keymap = crate::keymap::Keymap::build(&overrides);
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        (app, chord)
    }

    fn press_database_table_hotkey(app: &mut App) {
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
    }

    fn assert_active_database_table(app: &App, scenario: &str) {
        assert!(app.active_tab_is_database_table(), "{scenario} fixture must have the DatabaseTable surface active: active={}", app.active_tab);
    }

    fn added_database_rows(app: &App) -> usize {
        let Some(crate::app::EditorTabKind::DatabaseTable(_, state)) = app.tabs.get(app.active_tab).map(|tab| &tab.kind) else {
            panic!("active DatabaseTable fixture must retain its table state");
        };
        state.grid.added_rows.len()
    }

    #[test]
    fn database_table_chord_does_not_dispatch_behind_modal() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "modal");
        app.ide_panel.database.table_modal = Some(crate::app::database::DatabaseTableModal::RefreshPrompt {
            tab_id: crate::app::database::DatabaseTabId(7),
            close_after_save: false,
        });
        assert!(app.ide_panel.database.table_modal.is_some(), "modal fixture must be open: modal={:?}", app.ide_panel.database.table_modal);
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 0, "DatabaseTable chord must not add a row behind its modal: added={}", added_database_rows(&app));
    }

    #[test]
    fn database_table_chord_does_not_dispatch_while_text_field_focused() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "text field");
        let Some(crate::app::EditorTabKind::DatabaseTable(_, state)) = app.tabs.get_mut(app.active_tab).map(|tab| &mut tab.kind) else {
            panic!("active DatabaseTable fixture must retain its table state");
        };
        state.grid.focused_input = Some(crate::app::database::DatabaseTableInputTarget::Where);
        assert_eq!(state.grid.focused_input, Some(crate::app::database::DatabaseTableInputTarget::Where), "text input fixture must be focused: focused={:?}", state.grid.focused_input);
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 0, "DatabaseTable chord must not add a row while a text field owns input: added={}", added_database_rows(&app));
    }

    #[test]
    fn database_table_chord_does_not_dispatch_while_settings_is_open() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "Settings");
        app.show_settings = true;
        assert!(app.show_settings, "Settings fixture must be open: show_settings={}", app.show_settings);
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 0, "DatabaseTable chord must not add a row while Settings is open: added={}", added_database_rows(&app));
    }

    #[test]
    fn database_table_chord_dispatches_when_table_owns_input() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "positive control");
        assert!(app.ide_panel.database.table_modal.is_none(), "positive control must not have a modal: modal={:?}", app.ide_panel.database.table_modal);
        assert!(!app.show_settings, "positive control must have Settings closed: show_settings={}", app.show_settings);
        assert!(app.database_table_command_context_unowned(), "positive control must have an unowned table context: unowned={}", app.database_table_command_context_unowned());
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 1, "DatabaseTable chord must add a row when the table owns input: added={}", added_database_rows(&app));
    }

    #[test]
    fn settings_prevents_enter_from_committing_database_query_review() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            panic!("test app must initialize");
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        let mut tab = crate::app::app_behavior_tests::tab_with("SQL Console", None, "select 1");
        let query = crate::app::database::DatabaseQueryTabState {
            review: Some(crate::app::database::DatabaseQueryReviewState {
                transaction_id: crate::app::database::DatabaseTransactionId(2),
                sql: "select 1".to_string(),
                source_offset: 0,
                started_unix_ms: 1,
                deadline_unix_ms: 2,
                duration_ms: 1,
                returned_rows: 0,
                changed_rows: 0,
                mode: crate::app::database::DatabaseQueryMode::Run,
                finishing: false,
            }),
            ..crate::app::database::DatabaseQueryTabState::default()
        };
        tab.kind = crate::app::EditorTabKind::DatabaseQuery(
            crate::app::database::DatabaseQueryTabMeta {
                console_id: crate::app::database::SqlConsoleId(4),
                connection_id: crate::app::database::DatabaseConnectionId(3),
                database_name: "test".to_string(),
                title: "SQL Console".to_string(),
            },
            query,
        );
        app.tabs = vec![tab];
        app.active_tab = 0;
        app.show_settings = true;
        assert!(app.active_tab_is_database_query(), "review fixture must have the SQL Console surface active: active={}", app.active_tab);
        assert!(app.active_database_query_meta_state().is_some_and(|(_, state)| state.review.is_some()), "review fixture must start in SQL review: review={:?}", app.active_database_query_meta_state().map(|(_, state)| &state.review));
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::Enter),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
        assert!(app.active_database_query_meta_state().is_some_and(|(_, state)| state.review.is_some()), "Enter must not commit a SQL review behind Settings: review={:?}", app.active_database_query_meta_state().map(|(_, state)| &state.review));
    }

    fn terminal_open(panels: &crate::app::IdePanelState) -> bool {
        panels.is_open(crate::app::PanelId::Terminal)
    }

    fn relocated_top_terminal_with_explorer_open() -> crate::app::IdePanelState {
        let mut panels = crate::app::IdePanelState::default();
        let terminal = panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == crate::app::PanelId::Terminal)
            .unwrap();
        terminal.group = crate::app::PanelGroup::Top;
        terminal.open = false;
        panels.open(crate::app::PanelId::Explorer);
        panels
    }

    #[test]
    fn terminal_alt_q_opens_terminal_and_requests_spawn_when_missing() {
        let mut panels = crate::app::IdePanelState::default();

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, false);

        assert!(needs_spawn);
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_focuses_existing_closed_terminal_without_spawn() {
        let mut panels = crate::app::IdePanelState::default();

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, true);

        assert!(!needs_spawn);
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_opens_relocated_top_terminal_in_its_current_group() {
        let mut panels = relocated_top_terminal_with_explorer_open();

        assert!(panels.is_open(crate::app::PanelId::Explorer));
        assert!(!terminal_open(&panels));

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, true);

        assert!(!needs_spawn);
        assert!(terminal_open(&panels));
        assert!(!panels.is_open(crate::app::PanelId::Explorer));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_relocated_top_terminal_requests_spawn_when_missing() {
        let mut panels = relocated_top_terminal_with_explorer_open();

        assert!(panels.is_open(crate::app::PanelId::Explorer));
        assert!(!terminal_open(&panels));

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, false);

        assert!(needs_spawn);
        assert!(terminal_open(&panels));
        assert!(!panels.is_open(crate::app::PanelId::Explorer));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_toggles_focus_when_open() {
        let mut panels = crate::app::IdePanelState::default();
        panels.toggle(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;

        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(terminal_open(&panels));
        assert!(!panels.terminal_focused);

        panels.git.message_focused = true;
        panels.term_search_focused = true;
        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
        assert!(!panels.git.message_focused);
        assert!(!panels.term_search_focused);
    }

    #[test]
    fn terminal_alt_shift_q_closes_or_opens_without_focus_toggle() {
        let mut panels = crate::app::IdePanelState::default();
        panels.toggle(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;

        assert!(!apply_terminal_alt_q_shortcut(&mut panels, true, true));
        assert!(!terminal_open(&panels));
        assert!(!panels.terminal_focused);

        panels.git.message_focused = true;
        assert!(apply_terminal_alt_q_shortcut(&mut panels, true, false));
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
        assert!(!panels.git.message_focused);
    }

    #[test]
    fn terminal_alt_q_closes_bottom_peer_panel_before_opening_terminal() {
        let mut panels = crate::app::IdePanelState::default();
        panels.toggle(crate::app::PanelId::Problems);

        assert!(!panels.is_open(crate::app::PanelId::Terminal));
        assert!(panels.is_open(crate::app::PanelId::Problems));

        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(panels.terminal_focused);

        panels.toggle(crate::app::PanelId::Problems);
        assert!(!apply_terminal_alt_q_shortcut(&mut panels, true, true));
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(panels.terminal_focused);

        panels.toggle(crate::app::PanelId::Problems);
        assert!(apply_terminal_alt_q_shortcut(&mut panels, true, false));
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn remapped_terminal_close_tab_uses_focus_and_keymap_chord() {
        let overrides = crate::keymap::KeymapOverrides::from_value(serde_json::json!({
            "terminal.close_tab": ["alt+4"]
        }));
        let keymap = crate::keymap::Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let chord = crate::keymap::Chord::parse(crate::platform::PlatformKind::Linux, "alt+4").unwrap();
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;

        assert!(is_terminal_tab_close_shortcut(&panels) && keymap.hit(crate::keymap::Command::TerminalCloseTab, chord));

        panels.terminal_focused = false;
        assert!(!is_terminal_tab_close_shortcut(&panels) && keymap.hit(crate::keymap::Command::TerminalCloseTab, chord));

        panels.term_show_search = true;
        panels.term_search_focused = true;
        assert!(is_terminal_tab_close_shortcut(&panels) && keymap.hit(crate::keymap::Command::TerminalCloseTab, chord));
    }

    #[test]
    fn terminal_focus_only_allows_default_file_tree_chord_for_its_hit_command() {
        let overrides = crate::keymap::KeymapOverrides::from_value(serde_json::json!({
            "file_tree.paste": ["ctrl+z"]
        }));
        let keymap = crate::keymap::Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let chord = crate::keymap::Chord::parse(crate::platform::PlatformKind::Linux, "ctrl+z").unwrap();
        assert!(keymap.hit(crate::keymap::Command::FileTreePaste, chord));
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;
        assert!(super::input_owner::terminal_keyboard_owner(true, false, false, false, &panels));
        assert!(!default_file_tree_chord_for_hit(&keymap, chord));
    }

    #[test]
    fn reassigned_project_search_open_passes_through_terminal_focus() {
        let overrides = crate::keymap::KeymapOverrides::from_value(serde_json::json!({
            "search.project.open": ["ctrl+p"]
        }));
        let keymap = crate::keymap::Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let chord = crate::keymap::Chord::parse(crate::platform::PlatformKind::Linux, "ctrl+p").unwrap();
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;
        let terminal_owns_chord = super::input_owner::terminal_keyboard_owner(true, false, false, false, &panels);

        assert!(keymap.hit(crate::keymap::Command::SearchProjectOpen, chord));
        assert!(terminal_owns_chord);
        assert!(terminal_owns_chord
            && !super::input_owner::is_default_chord(crate::keymap::Command::SearchProjectOpen, chord, crate::platform::PlatformKind::Linux));
    }

    #[test]
    fn problems_alt_w_toggles_without_terminal_clickthrough_focus_mode() {
        let mut panels = crate::app::IdePanelState::default();

        apply_problems_alt_w_shortcut(&mut panels);
        assert!(panels.is_open(crate::app::PanelId::Problems));
        assert!(!panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.terminal_focused);
        assert!(panels.bottom_panel_blocks_editor_hover());

        apply_problems_alt_w_shortcut(&mut panels);
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(!panels.bottom_panel_blocks_editor_hover());
    }

    #[test]
    fn hover_keyboard_suppression_only_allows_escape_and_arrows() {
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::KeyC),
            true,
            false,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::KeyQ),
            false,
            true,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::AltLeft),
            false,
            false,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::Tab),
            false,
            true,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::KeyW),
            true,
            false,
        ));
        assert!(should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::Escape),
            false,
            false,
        ));
        assert!(should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::ArrowLeft),
            false,
            false,
        ));
    }

    fn markdown_toggle_for_test(
        panels: &crate::app::IdePanelState,
        show_settings: bool,
        show_search: bool,
        search_focused: bool,
        primary: bool,
        shift: bool,
        repeat: bool,
    ) -> Option<MarkdownGlobalToggleAction> {
        markdown_global_toggle_action(
            true,
            true,
            show_settings,
            show_search,
            search_focused,
            panels,
            PhysicalKey::Code(KeyCode::KeyV),
            primary,
            shift,
            repeat,
        )
    }

    fn stale_terminal_focus_state() -> crate::app::IdePanelState {
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;
        panels
    }

    #[test]
    fn markdown_global_toggle_matches_only_primary_shift_v_and_consumes_repeat() {
        let panels = crate::app::IdePanelState::default();
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode)
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, true),
            Some(MarkdownGlobalToggleAction::Consume)
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, false, false),
            None,
            "plain Primary+V must keep focused-field paste semantics"
        );
        assert_eq!(
            markdown_global_toggle_action(
                false,
                true,
                false,
                false,
                false,
                &panels,
                PhysicalKey::Code(KeyCode::KeyV),
                true,
                true,
                false,
            ),
            None,
            "non-Markdown documents must not gain a mode action"
        );
        assert_eq!(
            markdown_global_toggle_action(
                true,
                true,
                false,
                false,
                false,
                &panels,
                PhysicalKey::Code(KeyCode::KeyC),
                true,
                true,
                false,
            ),
            None
        );
    }

    #[test]
    fn markdown_global_toggle_uses_actual_terminal_keyboard_owner() {
        let mut panels = stale_terminal_focus_state();
        assert!(super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            None,
            "actual terminal body owner keeps terminal semantics"
        );

        panels.terminal_focused = false;
        panels.term_show_search = true;
        panels.term_search_focused = true;
        assert!(super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            None,
            "actual terminal-search owner keeps terminal-search semantics"
        );

        panels.term_search_focused = false;
        assert!(!super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "open terminal without keyboard ownership must not block Markdown toggle"
        );
    }

    #[test]
    fn markdown_global_toggle_beats_stale_terminal_focus_for_project_search() {
        let mut panels = stale_terminal_focus_state();
        panels.open(crate::app::PanelId::Search);
        panels.project_search.focused = Some(crate::app::project_search::ProjectSearchField::Query);

        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(
            panels.terminal_focused,
            "fixture must preserve stale terminal focus"
        );
        assert_eq!(
            panels.project_search.focused,
            Some(crate::app::project_search::ProjectSearchField::Query)
        );
        assert!(!super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "central route must consume Primary+Shift+V before project-search KeyV paste"
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, true),
            Some(MarkdownGlobalToggleAction::Consume),
            "repeat must remain consumed instead of reaching project-search paste"
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, false, false),
            None,
            "plain Primary+V remains available to project-search paste"
        );
    }

    #[test]
    fn markdown_global_toggle_beats_stale_terminal_focus_for_other_text_owners() {
        let mut global_search = stale_terminal_focus_state();
        assert_eq!(
            markdown_toggle_for_test(&global_search, false, true, true, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "global Search outranks stale terminal body focus"
        );

        global_search.term_show_search = true;
        global_search.term_search_focused = true;
        assert_eq!(
            markdown_toggle_for_test(&global_search, false, true, true, true, true, false),
            None,
            "actual terminal Search still outranks global Search"
        );

        let mut git = stale_terminal_focus_state();
        git.open(crate::app::PanelId::Git);
        git.git.message_focused = true;
        assert_eq!(
            markdown_toggle_for_test(&git, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "Git message focus must outrank stale terminal body focus"
        );

        let settings = stale_terminal_focus_state();
        assert_eq!(
            markdown_toggle_for_test(&settings, true, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "Settings keyboard ownership must outrank stale terminal body focus"
        );
    }

    #[test]
    fn file_tree_name_inputs_defer_to_global_toggle_but_hard_modals_keep_priority() {
        let mut panels = crate::app::IdePanelState::default();
        panels.file_tree_create_dialog = Some(crate::app::file_tree::FileTreeCreateDialog {
            kind: crate::app::file_tree::FileTreeCreateKind::File,
            parent_dir: std::path::PathBuf::from("/tmp"),
            editor: crate::editor::Editor::new(64),
            error: None,
        });
        assert!(super::input_owner::file_tree_text_input_owns_keyboard_context(&panels));

        panels.file_tree_delete_dialog = Some(crate::app::file_tree::FileTreeDeleteDialog {
            paths: vec![std::path::PathBuf::from("/tmp/example.md")],
            error: None,
        });
        assert!(
            !super::input_owner::file_tree_text_input_owns_keyboard_context(&panels),
            "hard confirmation must retain priority over the underlying name field"
        );
    }

    #[test]
    fn graph_and_owned_vcs_copy_route_precedes_git_message_input() {
        assert!(route_position(RouteId::GitCopySelection) < route_position(RouteId::GitMessage), "Git selection copy precedes Git message routing");
    }

    #[test]
    fn reassigned_editor_command_routes_through_main_keyboard_input() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else { return; };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs.push(crate::app::app_behavior_tests::tab_with(
            "hotkeys.rs",
            Some("/tmp/hotkeys.rs"),
            "",
        ));
        app.active_tab = 0;
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(
            crate::platform::CURRENT_PLATFORM,
            crate::keymap::Command::EditorGitDiffPrevHunk,
            chord,
        );
        app.keymap = crate::keymap::Keymap::build(&overrides);
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
        assert_eq!(app.readonly_notice_text, "Нет изменений Git в документе");
    }

    #[test]
    fn reassigned_sql_command_routes_through_main_keyboard_input() {
        fn sql_app() -> Option<App> {
            let text = "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment";
            let mut app = crate::app::app_behavior_tests::test_app()?;
            app.is_ide_mode = true;
            app.show_welcome = false;
            app.editor = crate::app::app_behavior_tests::editor_with(text);
            let mut tab = crate::app::app_behavior_tests::tab_with("SQL Console", None, text);
            tab.kind = crate::app::EditorTabKind::DatabaseQuery(
                crate::app::database::DatabaseQueryTabMeta {
                    console_id: crate::app::database::SqlConsoleId(7),
                    connection_id: crate::app::database::DatabaseConnectionId(3),
                    database_name: "postgres".to_string(),
                    title: "SQL Console".to_string(),
                },
                crate::app::database::DatabaseQueryTabState::default(),
            );
            app.tabs = vec![tab];
            app.active_tab = 0;
            Some(app)
        }

        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(
            crate::platform::CURRENT_PLATFORM,
            crate::keymap::Command::DatabaseQueryFormat,
            chord,
        );

        let Some(mut direct_app) = sql_app() else { return; };
        direct_app.keymap = crate::keymap::Keymap::build(&overrides);
        assert!(direct_app.active_tab_is_database_query());
        assert!(direct_app.run_bound_commands(Some(chord), None, false));
        assert_ne!(
            direct_app.editor.get_full_text(),
            "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment"
        );

        let Some(mut app) = sql_app() else { return; };
        app.keymap = crate::keymap::Keymap::build(&overrides);
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
        assert_ne!(
            app.editor.get_full_text(),
            "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment"
        );
    }

    #[test]
    fn markdown_toggle_precedes_later_text_field_routes() {
        assert!(route_position(RouteId::ConfirmModal) < route_position(RouteId::MarkdownToggle), "confirmation modal precedes applying the Markdown toggle");
        assert!(route_position(RouteId::FileTreeModal) < route_position(RouteId::MarkdownToggle), "file tree modal precedes applying the Markdown toggle");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::SettingsIgnoreField), "Markdown toggle precedes Settings text input");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::ProjectSearchField), "Markdown toggle precedes Project Search input");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::LspLogFilter), "Markdown toggle precedes LSP filter input");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::GitMessage), "Markdown toggle precedes Git message input");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::ApiClient), "Markdown toggle precedes API Client input");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::TerminalGate), "Markdown toggle precedes the terminal gate");
        assert!(route_position(RouteId::ApiClient) < route_position(RouteId::TerminalGate), "API Client input precedes the terminal gate");
        assert!(route_position(RouteId::MarkdownToggle) < route_position(RouteId::FinalRoute), "Markdown toggle precedes final terminal search, global search, and editor routing");
    }
