// Key route characterization tests (real key presses).
use super::*;

fn bind_chord(app: &mut App, command: crate::keymap::Command, value: &str) {
    let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, value)
        .expect("test chord parses");
    let mut overrides = crate::keymap::KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, command, chord);
    app.keymap = crate::keymap::Keymap::build(&overrides);
    app.modifiers = if value.contains("mod+") {
        let primary = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER
        } else {
            winit::keyboard::ModifiersState::CONTROL
        };
        if value.contains("shift+") {
            primary | winit::keyboard::ModifiersState::SHIFT
        } else {
            primary
        }
    } else {
        winit::keyboard::ModifiersState::empty()
    };
}

fn press_key(app: &mut App, physical_key: winit::keyboard::PhysicalKey) {
    let state = crate::app::events::host_loop::HeadlessLoopState::default();
    app.handle_main_key_input(
        &crate::app::events::host_loop::HostLoop::headless(&state),
        KeyInput {
            physical_key,
            logical_text: None,
            text: None,
            state: winit::event::ElementState::Pressed,
            repeat: false,
        },
    );
}

#[test]
fn markdown_toggle_consumes_chord_before_project_search_text_input() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(std::path::PathBuf::from("/tmp/char-c.md"));
    app.file_extension = "md".to_string();
    app.tabs = vec![crate::app::app_behavior_tests::tab_with(
        "char-c.md",
        Some("/tmp/char-c.md"),
        "# Note",
    )];
    app.active_tab = 0;
    app.editor = crate::app::app_behavior_tests::editor_with("# Note");
    app.ide_panel.open(crate::app::PanelId::Search);
    app.ide_panel.project_search.focused =
        Some(crate::app::project_search::ProjectSearchField::Query);
    app.ide_panel.project_search.query_editor.insert_str("keep this");
    let query_before = app.ide_panel.project_search.query_editor.get_full_text();
    bind_chord(
        &mut app,
        crate::keymap::Command::MarkdownToggleMode,
        "mod+shift+v",
    );

    press_key(
        &mut app,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV),
    );

    assert_eq!(app.markdown_mode(), crate::app::MarkdownMode::Read);
    assert_eq!(
        app.ide_panel.project_search.query_editor.get_full_text(),
        query_before
    );
}

#[test]
fn fps_toggle_chord_falls_through_while_terminal_has_keyboard_focus() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.ide_panel.open(crate::app::PanelId::Terminal);
    app.ide_panel.terminal_focused = true;
    bind_chord(&mut app, crate::keymap::Command::ViewToggleFps, "mod+f");
    let show_fps = app.show_fps;

    press_key(
        &mut app,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyF),
    );

    assert_eq!(app.show_fps, show_fps);
}

#[test]
fn tabs_switch_next_chord_changes_active_editor_tab() {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs = vec![
        crate::app::app_behavior_tests::tab_with("first.rs", None, "first"),
        crate::app::app_behavior_tests::tab_with("second.rs", None, "second"),
    ];
    app.active_tab = 0;
    app.editor = crate::app::app_behavior_tests::editor_with("first");
    bind_chord(
        &mut app,
        crate::keymap::Command::TabsSwitchNext,
        "mod+pagedown",
    );

    press_key(
        &mut app,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::PageDown),
    );

    assert_eq!(app.active_tab, 1);
    assert_eq!(app.editor.get_full_text(), "second");
}
