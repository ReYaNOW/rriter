use super::*;

fn press(app: &mut App, key: KeyCode, modifiers: winit::keyboard::ModifiersState) {
    app.modifiers = modifiers;
    let state = crate::app::events::host_loop::HeadlessLoopState::default();
    app.handle_main_key_input(
        &crate::app::events::host_loop::HostLoop::headless(&state),
        KeyInput {
            physical_key: PhysicalKey::Code(key),
            logical_text: None,
            text: None,
            state: ElementState::Pressed,
            repeat: false,
        },
    );
}

fn test_app() -> App {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app
}

fn hover_popup() -> crate::app::mouse::HoverPopup {
    crate::app::mouse::HoverPopup {
        text: "hover".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 0,
        anchor_x: 0.0,
        anchor_y: 0.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 1.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    }
}

#[test]
fn escape_clears_hover_and_closes_git_commit_menu() {
    let mut app = test_app();
    app.hover.popup = Some(hover_popup());
    app.ide_panel.git.commit_menu_opened_at = Some(std::time::Instant::now());

    press(&mut app, KeyCode::Escape, winit::keyboard::ModifiersState::empty());

    assert!(app.hover.popup.is_none());
    assert!(!app.ide_panel.git.commit_menu_open());
}

#[test]
fn escape_clears_hover_and_api_mock_constraint_menu() {
    let mut app = test_app();
    app.hover.popup = Some(hover_popup());
    app.ide_panel.api.mock_contract_constraint_menu =
        Some(crate::app::api_client::ApiMockContractConstraintMenu {
            route_idx: 0,
            group: crate::ui_system::ApiMockContractFieldGroup::Body,
            field_idx: 0,
        });

    press(&mut app, KeyCode::Escape, winit::keyboard::ModifiersState::empty());

    assert!(app.hover.popup.is_none());
    assert!(app.ide_panel.api.mock_contract_constraint_menu.is_none());
}

#[test]
fn escape_closes_inline_git_popup_before_git_commit_menu() {
    let mut app = test_app();
    app.inline_git_popup = Some(crate::app::InlineGitPopup {
        hunk_idx: 0,
        anchor_line: 0,
        lines: Vec::new(),
        spans: Vec::new(),
        diff_state: crate::app::git_diff::build_diff_view(String::new(), String::new()),
    });
    app.ide_panel.git.commit_menu_opened_at = Some(std::time::Instant::now());

    press(&mut app, KeyCode::Escape, winit::keyboard::ModifiersState::empty());

    assert!(app.inline_git_popup.is_none());
    assert!(app.ide_panel.git.commit_menu_open());
}

#[test]
fn escape_closes_project_search_help_before_git_commit_menu() {
    let mut app = test_app();
    app.ide_panel.project_search.help_open = true;
    app.ide_panel.git.commit_menu_opened_at = Some(std::time::Instant::now());

    press(&mut app, KeyCode::Escape, winit::keyboard::ModifiersState::empty());

    assert!(!app.ide_panel.project_search.help_open);
    assert!(app.ide_panel.git.commit_menu_open());
}

#[test]
fn escape_closes_database_context_menu_before_git_commit_menu() {
    let mut app = test_app();
    app.ide_panel.database.context_menu = Some(crate::app::database::DatabaseContextMenu {
        target: crate::app::database::DatabaseContextTarget::Connection(
            crate::app::database::DatabaseConnectionId(1),
        ),
        x: 0.0,
        y: 0.0,
        entries: Vec::new(),
        opened_at: std::time::Instant::now(),
    });
    app.ide_panel.git.commit_menu_opened_at = Some(std::time::Instant::now());

    press(&mut app, KeyCode::Escape, winit::keyboard::ModifiersState::empty());

    assert!(app.ide_panel.database.context_menu.is_none());
    assert!(app.ide_panel.git.commit_menu_open());
}
