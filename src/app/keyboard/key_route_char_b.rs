// Key route characterization tests (real key presses).

use super::*;

fn primary_alt_modifiers() -> winit::keyboard::ModifiersState {
    if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
        winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
    } else {
        winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
    }
}

fn press_key(app: &mut App, physical_key: KeyCode) {
    let state = crate::app::events::host_loop::HeadlessLoopState::default();
    app.handle_main_key_input(
        &crate::app::events::host_loop::HostLoop::headless(&state),
        KeyInput {
            physical_key: PhysicalKey::Code(physical_key),
            logical_text: None,
            text: None,
            state: ElementState::Pressed,
            repeat: false,
        },
    );
}

fn query_app(text: &str) -> App {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    let mut tab = crate::app::app_behavior_tests::tab_with("SQL Console", None, text);
    tab.kind = crate::app::EditorTabKind::DatabaseQuery(
        crate::app::database::DatabaseQueryTabMeta {
            console_id: crate::app::database::SqlConsoleId(7),
            connection_id: crate::app::database::DatabaseConnectionId(3),
            database_name: "test".to_string(),
            title: "SQL Console".to_string(),
        },
        crate::app::database::DatabaseQueryTabState::default(),
    );
    app.editor = crate::app::app_behavior_tests::editor_with(text);
    app.tabs = vec![tab];
    app.active_tab = 0;
    app
}

#[test]
fn recording_captures_enter_before_database_query_review() {
    let mut app = query_app("select 1");
    let Some(crate::app::EditorTabKind::DatabaseQuery(_, state)) = app.tabs.first_mut().map(|tab| &mut tab.kind) else {
        panic!("query tab fixture");
    };
    state.review = Some(crate::app::database::DatabaseQueryReviewState {
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
    });
    app.show_settings = true;
    app.settings_tab = 6;
    app.keymap_settings.begin_recording(crate::keymap::Command::DatabaseQueryFormat);

    press_key(&mut app, KeyCode::Enter);

    assert_eq!(app.keymap_settings.recording.map(|recording| recording.command), Some(crate::keymap::Command::DatabaseQueryFormat), "recording must retain ownership when Enter is rejected");
    assert_eq!(app.keymap_settings.hint, Some("зарезервировано"), "recording must report that Enter was captured and rejected");
    assert!(app.active_database_query_meta_state().is_some_and(|(_, state)| state.review.is_some()), "query review must remain pending");
}

#[test]
fn confirm_modal_blocks_api_client_command_chord() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.ide_panel.open(crate::app::PanelId::ApiClient);
    app.confirm_dialog = crate::app::ConfirmDialog::armed_with(crate::app::PendingAction::Quit);
    let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g").expect("test chord");
    let mut overrides = crate::keymap::KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::GitToggleGraph, chord);
    app.keymap = crate::keymap::Keymap::build(&overrides);
    app.modifiers = primary_alt_modifiers();

    press_key(&mut app, KeyCode::KeyG);

    assert!(app.modal_dialog_open(), "confirm modal must remain open");
    assert_eq!(app.ide_panel.git.bottom_pane, crate::app::git_panel::GitBottomPane::Closed, "API surface command must not run through modal");
}

#[test]
fn api_input_owner_consumes_chord_before_general_command_fallback() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.ide_panel.open(crate::app::PanelId::ApiClient);
    app.ide_panel.api.focused = Some(crate::app::api_client::ApiFocus::ImportUrl);
    let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g").expect("test chord");
    let mut overrides = crate::keymap::KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::GitToggleGraph, chord);
    app.keymap = crate::keymap::Keymap::build(&overrides);
    app.modifiers = primary_alt_modifiers();

    press_key(&mut app, KeyCode::KeyG);

    assert_eq!(app.ide_panel.api.focused, Some(crate::app::api_client::ApiFocus::ImportUrl), "API input retains keyboard ownership");
    assert_eq!(app.ide_panel.git.bottom_pane, crate::app::git_panel::GitBottomPane::Closed, "general command must not run through focused API input");
}

#[test]
fn database_table_command_wins_collision_with_api_command() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.ide_panel.open(crate::app::PanelId::ApiClient);
    let tab_id = crate::app::database::DatabaseTabId(7);
    let mut tab = crate::app::app_behavior_tests::tab_with("items", None, "");
    tab.kind = crate::app::EditorTabKind::DatabaseTable(
        crate::app::database::DatabaseTableTabMeta {
            tab_id,
            connection_id: crate::app::database::DatabaseConnectionId(3),
            database_name: "test".to_string(),
            table_name: "items".to_string(),
        },
        crate::app::database::DatabaseTableTabState {
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
        },
    );
    app.tabs = vec![tab];
    app.active_tab = 0;
    let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g").expect("test chord");
    let mut overrides = crate::keymap::KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::DatabaseTableAddRow, chord);
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::ApiSendRequest, chord);
    app.keymap = crate::keymap::Keymap::build(&overrides);
    app.modifiers = primary_alt_modifiers();

    press_key(&mut app, KeyCode::KeyG);

    let Some(crate::app::EditorTabKind::DatabaseTable(_, state)) = app.tabs.first().map(|tab| &tab.kind) else {
        panic!("active DatabaseTable fixture");
    };
    assert_eq!(state.grid.added_rows.len(), 1, "DatabaseTable command must add a row");
    assert_eq!(app.ide_panel.database.global_error, None, "API Send Request must not run");
}

#[test]
fn terminal_close_precedes_confirm_modal() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.ide_panel.open(crate::app::PanelId::Terminal);
    app.confirm_dialog = crate::app::ConfirmDialog::armed_with(crate::app::PendingAction::Quit);
    let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "alt+shift+q").expect("test chord");
    let mut overrides = crate::keymap::KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::TerminalClose, chord);
    app.keymap = crate::keymap::Keymap::build(&overrides);
    app.modifiers = winit::keyboard::ModifiersState::ALT | winit::keyboard::ModifiersState::SHIFT;

    press_key(&mut app, KeyCode::KeyQ);

    // characterization: current behaviour
    assert!(!app.ide_panel.is_open(crate::app::PanelId::Terminal), "terminal close currently runs before the confirm modal");
    assert!(app.modal_dialog_open(), "confirm modal remains open after the terminal closes");
}

#[test]
fn database_query_run_wins_collision_with_general_command() {
    let mut app = query_app("select 1");
    app.ide_panel.database.pending_job = Some(crate::app::database::DatabasePendingJob {
        id: crate::app::database::DatabaseJobId(1),
        kind: crate::app::database::DatabasePendingJobKind::RunUserSql,
        owner: crate::app::database::DatabaseJobOwner::Query(crate::app::database::SqlConsoleId(7)),
        connection_id: crate::app::database::DatabaseConnectionId(3),
        database_name: Some("test".to_string()),
        table_name: None,
    });
    let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g").expect("test chord");
    let mut overrides = crate::keymap::KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::DatabaseQueryRun, chord);
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::GitToggleGraph, chord);
    app.keymap = crate::keymap::Keymap::build(&overrides);
    app.modifiers = primary_alt_modifiers();

    press_key(&mut app, KeyCode::KeyG);

    assert_eq!(app.ide_panel.database.global_error.as_deref(), Some("Сейчас уже выполняется запрос к базе данных. Отмените его или дождитесь завершения."), "query run path must observe the pending job");
    assert_eq!(app.ide_panel.git.bottom_pane, crate::app::git_panel::GitBottomPane::Closed, "general Git command must not run");
}
