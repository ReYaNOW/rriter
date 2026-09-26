use super::*;

fn sidebar_database_app_with_restored_expansion() -> App {
    use crate::app::database::{
        DatabaseConnectionConfig, DatabaseConnectionId, DatabaseConnectionNode, DatabaseJobId,
        DatabaseJobOwner, DatabasePendingJob, DatabasePendingJobKind,
    };
    let mut app = crate::app::reviewer_stage2_test_app().expect("headless App");
    app.is_ide_mode = true;
    let mut connection = DatabaseConnectionNode::new(DatabaseConnectionConfig {
        id: DatabaseConnectionId(1),
        display_name: "Connection 1".to_string(),
        username: "postgres".to_string(),
        ..DatabaseConnectionConfig::default()
    });
    connection.expanded = true;
    app.ide_panel.database.connections.push(connection);
    // Busy runtime slot: a started load is queued instead of touching the network.
    app.ide_panel.database.pending_job = Some(DatabasePendingJob {
        id: DatabaseJobId(900),
        kind: DatabasePendingJobKind::LoadDdl,
        owner: DatabaseJobOwner::Connection(DatabaseConnectionId(1)),
        connection_id: DatabaseConnectionId(1),
        database_name: None,
        table_name: None,
    });
    app
}

#[test]
fn sidebar_click_opening_database_starts_restored_expanded_catalog_load() {
    use crate::app::PanelId;
    use crate::app::database::{DatabaseConnectionChildrenState, DatabasePendingJobKind};
    let mut app = sidebar_database_app_with_restored_expansion();
    assert!(!app.ide_panel.is_open(PanelId::Database));
    assert_eq!(
        app.ide_panel.database.connections[0].children_state(),
        DatabaseConnectionChildrenState::ExpandedUnloaded
    );

    app.toggle_sidebar_panel_from_click(PanelId::Database);

    assert!(app.ide_panel.is_open(PanelId::Database));
    assert_eq!(
        app.ide_panel.database.connections[0].children_state(),
        DatabaseConnectionChildrenState::ExpandedLoading
    );
    assert_eq!(app.ide_panel.database.queued_commands.len(), 1);
    assert_eq!(
        app.ide_panel.database.queued_commands[0].1.kind,
        DatabasePendingJobKind::LoadDatabases
    );

    // Closing never starts another load.
    app.toggle_sidebar_panel_from_click(PanelId::Database);
    assert!(!app.ide_panel.is_open(PanelId::Database));
    assert_eq!(app.ide_panel.database.queued_commands.len(), 1);
}

#[test]
fn sidebar_click_closing_database_or_opening_explorer_does_not_load_catalog() {
    use crate::app::PanelId;
    use crate::app::database::DatabaseConnectionChildrenState;
    let mut app = sidebar_database_app_with_restored_expansion();
    app.ide_panel.toggle(PanelId::Database);
    assert!(app.ide_panel.is_open(PanelId::Database));

    app.toggle_sidebar_panel_from_click(PanelId::Database);
    assert!(!app.ide_panel.is_open(PanelId::Database));
    app.ide_panel.toggle(PanelId::Database);

    // Explorer shares the Top group: opening it closes Database without loading.
    app.toggle_sidebar_panel_from_click(PanelId::Explorer);
    assert!(app.ide_panel.is_open(PanelId::Explorer));
    assert!(!app.ide_panel.is_open(PanelId::Database));
    assert!(app.ide_panel.database.queued_commands.is_empty());
    assert_eq!(
        app.ide_panel.database.connections[0].children_state(),
        DatabaseConnectionChildrenState::ExpandedUnloaded
    );
}

fn git_state_with_owned_selection() -> crate::app::git_panel::GitPanelState {
    let mut git = crate::app::git_panel::GitPanelState::default();
    git.toggle_logs_pane();
    git.seed_git_log_for_test("selected");
    let line = git.git_logs.display_line_at(0).unwrap();
    assert!(git.git_logs.set_selection(
        crate::app::git_panel::GitLogTextPoint {
            line: line.id(),
            byte: 0,
        },
        crate::app::git_panel::GitLogTextPoint {
            line: line.id(),
            byte: line.byte_len(),
        },
    ));
    git.claim_git_logs_copy_owner();
    git
}

#[test]
fn git_logs_text_selection_handoff_clears_prior_text_focus_and_claims_owner() {
    let mut app = crate::app::app_behavior_tests::test_app().unwrap();
    app.ide_panel.open(crate::app::PanelId::Git);
    app.ide_panel.git.toggle_logs_pane();
    app.ide_panel.git.seed_git_log_for_test("selected");
    app.ide_panel.git.message_focused = true;
    app.show_search = true;
    app.search_focused = true;
    let line = app.ide_panel.git.git_logs.display_line_at(0).unwrap();
    let point = crate::app::git_panel::GitLogTextPoint {
        line: line.id(),
        byte: 0,
    };

    assert!(begin_git_logs_text_selection(&mut app, point));

    assert!(!app.ide_panel.git.message_focused);
    assert!(!app.search_focused);
    assert!(app.ide_panel.git.owns_git_logs_copy());
    assert!(app.ide_panel.git.git_logs.selection().is_some());
}

#[test]
fn git_logs_copy_owner_persists_without_new_left_press_and_inside_console() {
    let mut git = git_state_with_owned_selection();
    assert_eq!(
        git.copy_owned_git_logs_selection().as_deref(),
        Some("selected")
    );

    // Mouse release/move do not run the left-press ownership transition.
    assert_eq!(
        git.copy_owned_git_logs_selection().as_deref(),
        Some("selected")
    );

    update_git_logs_copy_owner_on_left_press(
        &mut git,
        Some(crate::ui_system::UiId::GitLogsScroll),
    );
    assert!(git.owns_git_logs_copy());
    update_git_logs_copy_owner_on_left_press(
        &mut git,
        Some(crate::ui_system::UiId::GitLogsBody),
    );
    assert!(git.owns_git_logs_copy());
}

#[test]
fn git_logs_copy_owner_is_revoked_by_other_copy_capable_focus_targets() {
    for target in [
        crate::ui_system::UiId::GitMessageInput,
        crate::ui_system::UiId::EditorTextBody,
        crate::ui_system::UiId::ApiBodyInput(0),
        crate::ui_system::UiId::LspLogArea(0),
    ] {
        let mut git = git_state_with_owned_selection();
        update_git_logs_copy_owner_on_left_press(&mut git, Some(target));
        assert!(
            !git.owns_git_logs_copy(),
            "target {target:?} must revoke stale VCS copy ownership"
        );
        assert_eq!(git.copy_owned_git_logs_selection(), None);
    }
}

#[test]
fn markdown_mode_toggle_target_preserves_only_main_vertical_click_stop() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        return;
    };
    app.scroll_y.current = 40.0;
    app.scroll_y.target = 140.0;
    app.scroll_y.velocity = 22.0;
    app.scroll_y.anim_speed = 7.0;
    app.scroll_x.current = 8.0;
    app.scroll_x.target = 28.0;
    app.scroll_x.velocity = 6.0;

    stop_click_scroll_anims(&mut app, true);

    assert_eq!(app.scroll_y.current, 40.0);
    assert_eq!(app.scroll_y.target, 140.0);
    assert_eq!(app.scroll_y.velocity, 22.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
    assert_eq!(app.scroll_x.current, 8.0);
    assert_eq!(app.scroll_x.target, 8.0);
    assert_eq!(app.scroll_x.velocity, 0.0);

    stop_click_scroll_anims(&mut app, false);
    assert_eq!(app.scroll_y.current, 40.0);
    assert_eq!(app.scroll_y.target, 40.0);
    assert_eq!(app.scroll_y.velocity, 0.0);
}

#[test]
fn markdown_mode_toggle_click_stop_then_action_preserves_inertia() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        return;
    };
    app.show_welcome = false;
    app.file_path = Some(std::path::PathBuf::from("/tmp/readme.md"));
    app.file_extension = "md".to_string();
    app.editor = crate::app::app_behavior_tests::editor_with("# title\n\nbody\n");
    app.scroll_y.current = 64.0;
    app.scroll_y.target = 144.0;
    app.scroll_y.velocity = 35.0;
    app.scroll_y.anim_speed = 7.0;
    app.ui_registry.register_blocker(
        crate::ui_system::UiId::MarkdownModeToggle,
        10.0,
        10.0,
        80.0,
        30.0,
        20.0,
        20.0,
    );

    let preserve = preserve_main_vertical_scroll_for_click(&app, 20.0, 20.0);
    stop_click_scroll_anims(&mut app, preserve);
    app.handle_ui_click(crate::ui_system::UiId::MarkdownModeToggle);

    assert_eq!(app.markdown_mode(), crate::app::MarkdownMode::Read);
    assert_eq!(app.scroll_y.current, 64.0);
    assert_eq!(app.scroll_y.target, 144.0);
    assert_eq!(app.scroll_y.velocity, 35.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
    assert!(app.markdown.scroll_transition.is_some());
}

#[test]
fn markdown_mode_toggle_click_exception_uses_topmost_registry_target() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        return;
    };
    app.ui_registry.register_blocker(
        crate::ui_system::UiId::MarkdownModeToggle,
        10.0,
        10.0,
        80.0,
        30.0,
        20.0,
        20.0,
    );
    assert!(preserve_main_vertical_scroll_for_click(&app, 20.0, 20.0));

    app.ui_registry.mark_overlay_start();
    app.ui_registry.register_blocker(
        crate::ui_system::UiId::BottomPanelBody,
        0.0,
        0.0,
        200.0,
        100.0,
        20.0,
        20.0,
    );
    assert_eq!(
        app.ui_registry.find_overlay_at(20.0, 20.0),
        Some(crate::ui_system::UiId::BottomPanelBody)
    );
    assert!(!preserve_main_vertical_scroll_for_click(&app, 20.0, 20.0));
}

#[test]
fn markdown_reader_left_release_finishes_before_ui_dispatch_and_preserves_range() {
    let mut markdown = crate::app::markdown::MarkdownTabState::default();
    markdown.read_source = "abcdef".to_string();
    markdown.begin_read_selection(1);
    markdown.update_read_selection(5);
    assert!(markdown.read_selecting);

    let release_target = crate::ui_system::UiId::BottomPanelBody;
    assert_ne!(release_target, crate::ui_system::UiId::MarkdownReadBody);
    assert!(finish_markdown_read_selection_on_left_release(
        &mut markdown,
        ElementState::Released,
        winit::event::MouseButton::Left,
    ));

    assert!(!markdown.read_selecting);
    assert_eq!(markdown.read_selection_range(), Some(1..5));
    assert!(!finish_markdown_read_selection_on_left_release(
        &mut markdown,
        ElementState::Released,
        winit::event::MouseButton::Left,
    ));
    assert!(!markdown.read_selecting);
}

#[test]
fn hover_rect_helpers_union_and_padding_are_inclusive() {
    assert_eq!(union_rect(None, None), None);
    assert_eq!(
        union_rect(Some((1.0, 2.0, 3.0, 4.0)), None),
        Some((1.0, 2.0, 3.0, 4.0)),
    );
    assert_eq!(
        union_rect(
            Some((10.0, 10.0, 20.0, 20.0)),
            Some((25.0, 5.0, 20.0, 10.0)),
        ),
        Some((10.0, 5.0, 35.0, 25.0)),
    );
    assert!(point_in_padded_rect(
        8.0,
        8.0,
        (10.0, 10.0, 20.0, 20.0),
        2.0,
    ));
    assert!(!point_in_padded_rect(
        7.9,
        8.0,
        (10.0, 10.0, 20.0, 20.0),
        2.0,
    ));
}

#[test]
fn terminal_mouse_helpers_match_sgr_protocol_edges() {
    assert_eq!(
        terminal_mouse_button_code(winit::event::MouseButton::Left),
        0,
    );
    assert_eq!(
        terminal_mouse_button_code(winit::event::MouseButton::Middle),
        1,
    );
    assert_eq!(
        terminal_mouse_button_code(winit::event::MouseButton::Right),
        2,
    );
    assert_eq!(terminal_mouse_cell_x(0.0, 50.0, 10.0), 1);
    assert_eq!(terminal_mouse_cell_x(75.0, 50.0, 10.0), 3);
    assert_eq!(
        terminal_mouse_cell_y(172.0, 100.0, 100.0, 0.0, 20.0, 1.0, 5),
        4,
    );
    assert_eq!(terminal_mouse_sgr_sequence(0, 3, 4, true), "\x1b[<0;3;4M",);
    assert_eq!(terminal_mouse_sgr_sequence(2, 1, 1, false), "\x1b[<2;1;1m",);
}

#[test]
fn autocomplete_scroll_click_target_keeps_thumb_or_pages_to_pointer() {
    assert_eq!(
        autocomplete_scroll_click_target(20.0, 10.0, 200.0, 0.0, 3, 1.0),
        None,
    );

    let (drag_offset, target) =
        autocomplete_scroll_click_target(13.0, 10.0, 160.0, 0.0, 20, 1.0).unwrap();
    assert_eq!(drag_offset, 0.0);
    assert_eq!(target, 0.0);

    let (_, paged_target) =
        autocomplete_scroll_click_target(140.0, 10.0, 160.0, 0.0, 20, 1.0).unwrap();
    assert!(paged_target > 0.0);
}

#[test]
fn autocomplete_scroll_drag_updates_target_without_teleporting_current() {
    let mut scroll = crate::scroll::ScrollState::new(7.0);
    scroll.current = 12.0;
    scroll.target = 20.0;
    scroll.velocity = 9.0;
    apply_autocomplete_scroll_drag(&mut scroll, 144.0, 8.0);
    assert_eq!(scroll.current, 12.0);
    assert_eq!(scroll.target, 144.0);
    assert_eq!(scroll.velocity, 9.0);
    assert_eq!(scroll.drag_offset, 8.0);
    assert!(scroll.is_dragging);
    assert_eq!(scroll.anim_speed, 15.0);
}

#[test]
fn click_stop_then_drag_cleanup_clears_old_motion_without_snapping_destination() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        return;
    };
    app.scroll_x.current = 12.0;
    app.scroll_x.target = 72.0;
    app.scroll_x.velocity = 14.0;
    app.scroll_x.anim_speed = 7.0;

    stop_click_scroll_anims(&mut app, false);
    assert_eq!(app.scroll_x.current, 12.0);
    assert_eq!(app.scroll_x.target, 12.0);
    assert_eq!(app.scroll_x.velocity, 0.0);

    assert!(crate::app::mouse::apply_scrollbar_drag_target(
        &mut app.scroll_x,
        144.0,
        9.0,
    ));
    assert_eq!(app.scroll_x.current, 12.0);
    assert_eq!(app.scroll_x.target, 144.0);
    assert_eq!(app.scroll_x.velocity, 0.0);

    app.cancel_pointer_interactions();
    assert!(!app.scroll_x.is_dragging);
    assert_eq!(app.scroll_x.drag_offset, 0.0);
    assert_eq!(app.scroll_x.current, 12.0);
    assert_eq!(app.scroll_x.target, 144.0);
}

#[test]
fn autocomplete_item_index_at_ignores_scrollbar_and_accounts_for_scroll() {
    let rect = (10.0, 20.0, 200.0, 260.0);
    assert_eq!(
        autocomplete_item_index_at(20.0, 20.0, rect, 0.0, 10, 1.0),
        Some(0)
    );
    assert_eq!(
        autocomplete_item_index_at(20.0, 60.0, rect, 0.0, 10, 1.0),
        Some(1)
    );
    assert_eq!(
        autocomplete_item_index_at(20.0, 60.0, rect, 72.0, 10, 1.0),
        Some(3)
    );
    assert_eq!(
        autocomplete_item_index_at(202.0, 60.0, rect, 0.0, 10, 1.0),
        None
    );
}

#[test]
fn mouse_lsp_action_route_delegates_to_central_action_path() {
    let source = include_str!("mouse_declarative_press.rs");
    let start = source
        .find("if let Some(menu) = self.lsp_actions_menu.as_ref()")
        .expect("LSP mouse menu route");
    let end = source[start..]
        .find("// Глобальная обработка декларативного UI")
        .map(|offset| start + offset)
        .expect("end of LSP mouse menu route");
    let route = &source[start..end];

    assert!(route.contains("menu.selected = idx;"));
    assert!(route.contains("self.apply_selected_lsp_action();"));
    assert!(!route.contains("LspActionItem::"));
    assert!(!route.contains("request_fix_all"));
    assert!(!route.contains("request_organize_imports"));
}
