    use super::*;
    use crate::app::app_behavior_tests::{editor_with, tab_with, test_app};
    use std::path::PathBuf;

    #[test]
    fn markdown_tab_state_default_is_edit_and_parser_is_lazy() {
        let state = MarkdownTabState::default();
        assert_eq!(state.mode, MarkdownMode::Edit);
        assert!(state.read_parser.is_none());
        assert_eq!(state.parser_creation_count(), 0);
    }

    #[test]
    fn non_markdown_tab_never_creates_markdown_parser() {
        let tab = tab_with("note.txt", Some("/tmp/note.txt"), "plain text\n");
        assert!(tab.markdown.read_parser.is_none());
        assert_eq!(tab.markdown.parser_creation_count(), 0);

        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/note.txt"));
        app.file_extension = "txt".to_string();
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown.mode, MarkdownMode::Edit);
        assert!(app.markdown.read_parser.is_none());
        assert_eq!(app.markdown.parser_creation_count(), 0);
    }

    #[test]
    fn markdown_edit_tab_keeps_parser_lazy_until_first_read() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n");

        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert!(app.markdown.read_parser.is_none());
        assert_eq!(app.markdown.parser_creation_count(), 0);

        app.set_markdown_mode(MarkdownMode::Read);
        assert!(app.markdown.read_parser.is_some());
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 1);
        assert!(app.markdown.read_document(app.editor.version).is_some());
    }

    #[test]
    fn markdown_unchanged_read_toggle_reuses_model_without_refresh() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n\ntext 😀\n");

        app.set_markdown_mode(MarkdownMode::Read);
        let version = app.editor.version;
        let model_ptr = app
            .markdown
            .read_document(version)
            .map(|document| document as *const _)
            .expect("first read model");
        assert_eq!(app.markdown.semantic_refresh_count(), 1);
        assert_eq!(app.markdown.parser_creation_count(), 1);

        app.set_markdown_mode(MarkdownMode::Edit);
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown.semantic_refresh_count(), 1);
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.read_model_version, Some(version));
        assert_eq!(
            app.markdown
                .read_document(version)
                .map(|document| document as *const _),
            Some(model_ptr)
        );
    }

    #[test]
    fn markdown_incremental_refresh_reuses_existing_parser() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n\ntext\n");
        app.set_markdown_mode(MarkdownMode::Read);
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 1);

        app.set_markdown_mode(MarkdownMode::Edit);
        app.editor.cursor = app.editor.len();
        let _ = app.editor.insert_str("\n> новое 😀\n");
        let edited_version = app.editor.version;
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 2);
        assert_eq!(app.markdown.read_model_version, Some(edited_version));
        assert!(app.markdown.read_document(edited_version).is_some());
    }

    #[test]
    fn markdown_missing_model_retries_with_fresh_parser() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n");
        app.set_markdown_mode(MarkdownMode::Read);
        let version = app.editor.version;
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 1);

        // This is the state left by refresh_read_model after a parse failure:
        // version/source are known, but the model and parser are cleared.
        app.markdown.read_model = None;
        app.markdown.read_parser = None;
        app.set_markdown_mode(MarkdownMode::Edit);
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown.parser_creation_count(), 2);
        assert_eq!(app.markdown.semantic_refresh_count(), 2);
        assert!(app.markdown.read_document(version).is_some());
    }

    #[test]
    fn markdown_read_wheel_routes_to_shared_scroll_and_blocks_other_targets() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll.jump_to(40.0);

        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownReadBody),
                false,
                &mut scroll,
                Some(500.0),
                80.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert!(scroll.target > 40.0);
        let before_scrollbar_wheel = scroll.target;
        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownReadScrollbar),
                false,
                &mut scroll,
                Some(500.0),
                80.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert!(scroll.target > before_scrollbar_wheel);
        let before_copy_button_wheel = scroll.target;
        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownCodeCopy(123)),
                false,
                &mut scroll,
                Some(500.0),
                80.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert!(scroll.target > before_copy_button_wheel);
        let before_code_scrollbar_wheel = scroll.target;
        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownCodeScrollbarX(123)),
                false,
                &mut scroll,
                Some(500.0),
                80.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert!(scroll.target > before_code_scrollbar_wheel);

        let read_before = (scroll.current, scroll.target);
        for hovered in [
            None,
            Some(crate::ui_system::UiId::EditorTab(0)),
            Some(crate::ui_system::UiId::StatusBar),
        ] {
            assert_eq!(
                handle_markdown_read_wheel(
                    MarkdownMode::Read,
                    hovered,
                    false,
                    &mut scroll,
                    Some(500.0),
                    80.0,
                ),
                MarkdownReadWheelResult::Blocked
            );
        }
        assert_eq!((scroll.current, scroll.target), read_before);
    }

    #[test]
    fn markdown_read_wheel_does_not_clamp_to_zero_while_bounds_are_unknown() {
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 240.0;
        scroll.target = 300.0;

        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownReadBody),
                false,
                &mut scroll,
                None,
                36.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert_eq!(scroll.current, 240.0);
        assert_eq!(scroll.target, 336.0);
        assert_eq!(scroll.anim_speed, 7.0);
    }

    #[test]
    fn markdown_mode_api_ends_thumb_drag_but_preserves_vertical_inertia() {
        let Some(mut app) = markdown_source_app("# one\n\ntwo\n") else {
            return;
        };
        app.scroll_y.current = 80.0;
        app.scroll_y.target = 150.0;
        app.scroll_y.velocity = 27.0;
        app.scroll_y.anim_speed = 7.0;
        app.scroll_y.is_dragging = true;
        app.scroll_y.drag_offset = 6.0;

        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        assert_eq!(app.scroll_y.current, 80.0);
        assert_eq!(app.scroll_y.target, 150.0);
        assert_eq!(app.scroll_y.velocity, 27.0);
        assert_eq!(app.scroll_y.anim_speed, 7.0);
        assert!(!app.scroll_y.is_dragging);
        assert_eq!(app.scroll_y.drag_offset, 0.0);
        assert!(app.markdown.scroll_transition.is_some());
    }

    #[test]
    fn markdown_mode_noop_does_not_touch_shared_scroll_or_pending_state() {
        let Some(mut app) = markdown_source_app("# one\n") else {
            return;
        };
        app.scroll_y.current = 21.5;
        app.scroll_y.target = 84.0;
        app.scroll_y.velocity = -11.0;
        app.scroll_y.anim_speed = 7.0;

        app.set_markdown_mode(MarkdownMode::Edit);

        assert_eq!(app.scroll_y.current, 21.5);
        assert_eq!(app.scroll_y.target, 84.0);
        assert_eq!(app.scroll_y.velocity, -11.0);
        assert_eq!(app.scroll_y.anim_speed, 7.0);
        assert!(app.markdown.scroll_transition.is_none());
    }

    #[test]
    fn markdown_edit_wheel_keeps_normal_source_route_available() {
        let mut read_scroll = crate::scroll::ScrollState::new(15.0);
        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Edit,
                Some(crate::ui_system::UiId::MarkdownReadBody),
                false,
                &mut read_scroll,
                Some(500.0),
                80.0,
            ),
            MarkdownReadWheelResult::NotRead
        );
    }

    #[test]
    fn markdown_unresolved_round_trip_preserves_shared_scroll_motion_and_source_state() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.show_welcome = false;
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# Заголовок 😀\n\nТекст **strong**.\n");
        app.editor.cursor = "# Заголовок".len();
        app.editor.selection_anchor = Some(0);
        app.scroll_y.current = 213.0;
        app.scroll_y.target = 287.0;
        app.scroll_y.velocity = 31.0;
        app.scroll_y.anim_speed = 7.0;
        app.scroll_x.jump_to(37.0);

        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        let cursor = app.editor.cursor;
        let selection = app.editor.selection_anchor;
        let scroll_before = (
            app.scroll_y.current,
            app.scroll_y.target,
            app.scroll_y.velocity,
            app.scroll_y.anim_speed,
            app.scroll_x.current,
            app.scroll_x.target,
        );

        app.set_markdown_mode(MarkdownMode::Read);
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        assert!(app.markdown.read_document(version).is_some());
        assert!(app.markdown.scroll_transition.is_some());
        assert_eq!(
            (
                app.scroll_y.current,
                app.scroll_y.target,
                app.scroll_y.velocity,
                app.scroll_y.anim_speed,
                app.scroll_x.current,
                app.scroll_x.target,
            ),
            scroll_before
        );

        app.toggle_markdown_mode();
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert!(app.markdown.scroll_transition.is_none());
        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert_eq!(app.editor.cursor, cursor);
        assert_eq!(app.editor.selection_anchor, selection);
        assert_eq!(
            (
                app.scroll_y.current,
                app.scroll_y.target,
                app.scroll_y.velocity,
                app.scroll_y.anim_speed,
                app.scroll_x.current,
                app.scroll_x.target,
            ),
            scroll_before
        );
    }

    #[test]
    fn markdown_mode_pending_and_shared_scroll_are_independent_between_tabs() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs = vec![
            tab_with("a.md", Some("/tmp/a.md"), "# A\n"),
            tab_with("b.md", Some("/tmp/b.md"), "# B\n"),
        ];
        app.active_tab = 0;
        app.sync_active_tab();

        app.set_markdown_mode(MarkdownMode::Read);
        app.scroll_y.jump_to(91.0);
        assert!(app.markdown.scroll_transition.is_some());
        app.sync_active_tab();

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.scroll_y.current, 0.0);
        assert!(app.markdown.scroll_transition.is_none());
        app.scroll_y.jump_to(17.0);
        app.sync_active_tab();

        app.active_tab = 0;
        app.sync_active_tab();
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        assert_eq!(app.scroll_y.current, 91.0);
        assert!(app.markdown.scroll_transition.is_some());
        app.sync_active_tab();

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.scroll_y.current, 17.0);
        assert!(app.markdown.scroll_transition.is_none());
    }

    #[test]
    fn markdown_preview_click_does_not_move_hidden_source_cursor_or_selection() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("abcdef\n");
        app.editor.cursor = 5;
        app.editor.selection_anchor = Some(2);
        app.is_dragging = true;
        app.is_editor_drag_pending = true;

        app.handle_ui_click(crate::ui_system::UiId::MarkdownReadBody);

        assert_eq!(app.editor.cursor, 5);
        assert_eq!(app.editor.selection_anchor, Some(2));
        assert!(!app.is_dragging);
        assert!(!app.is_editor_drag_pending);
    }

    #[test]
    fn markdown_reader_document_focus_handoff_clears_keyboard_owners_and_preserves_source_state() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.show_welcome = false;
        app.is_ide_mode = true;
        app.editor = editor_with("source text\n");
        app.editor.cursor = 8;
        app.editor.selection_anchor = Some(2);
        app.markdown.mode = MarkdownMode::Read;
        app.markdown.read_source = "source text\n".to_string();

        app.ide_panel.open(crate::app::PanelId::Terminal);
        app.ide_panel.open(crate::app::PanelId::ApiClient);
        app.ide_panel.terminal_focused = true;
        app.ide_panel.term_show_search = true;
        app.ide_panel.term_search_focused = true;
        app.show_search = true;
        app.search_focused = true;
        app.ide_panel.file_tree_focused = true;
        app.ide_panel.lsp_logs_focused = Some("rust-analyzer".to_string());
        app.ide_panel.lsp_log_filter_focused = true;
        app.ide_panel.git.message_focused = true;
        app.settings_ignore_focused = true;
        app.ide_panel.api.route_filter = "old".to_string();
        app.ide_panel.api.input_editor = editor_with("new filter");
        app.ide_panel.api.focused = Some(crate::app::api_client::ApiFocus::RouteFilter);

        let hidden_source = (app.editor.cursor, app.editor.selection_anchor);
        assert!(!app.editor_has_input_focus());

        app.focus_document_text_surface();
        app.markdown.begin_read_selection(1);
        app.markdown.update_read_selection(6);

        assert!(!app.ide_panel.terminal_focused);
        assert!(app.ide_panel.is_open(crate::app::PanelId::Terminal));
        assert!(!app.ide_panel.term_search_focused);
        assert!(!app.search_focused);
        assert!(app.show_search);
        assert!(!app.ide_panel.file_tree_focused);
        assert!(app.ide_panel.lsp_logs_focused.is_none());
        assert!(!app.ide_panel.lsp_log_filter_focused);
        assert!(!app.ide_panel.git.message_focused);
        assert!(!app.settings_ignore_focused);
        assert!(app.ide_panel.api.focused.is_none());
        assert_eq!(app.ide_panel.api.route_filter, "new filter");
        assert!(app.ide_panel.is_open(crate::app::PanelId::ApiClient));
        assert_eq!(
            (app.editor.cursor, app.editor.selection_anchor),
            hidden_source
        );
        assert_eq!(app.markdown.read_selection_range(), Some(1..6));
        assert!(app.markdown.read_selecting);
        assert!(app.editor_has_input_focus());
    }

    #[test]
    fn markdown_reader_reverse_selection_is_independent_from_hidden_editor_selection() {
        let Some(mut app) = markdown_source_app("# Heading\n\nText **strong** with `code λ`.\n")
        else {
            return;
        };
        app.editor.cursor = 7;
        app.editor.selection_anchor = Some(2);
        let source = app.editor.get_full_text();
        let hidden = (app.editor.cursor, app.editor.selection_anchor);
        assert!(
            app.markdown
                .refresh_read_model(app.editor.version, source.clone())
        );
        app.markdown.read_layout =
            crate::render_view::markdown_read::build_test_markdown_read_layout(&source, 500.0);

        app.markdown.begin_read_selection(source.len());
        app.markdown.update_read_selection(0);
        app.markdown.finish_read_selection();

        assert_eq!(app.markdown.read_selection_range(), Some(0..source.len()));
        let copied = app
            .markdown
            .selected_read_text()
            .expect("reader selection text");
        assert!(copied.contains("Heading"));
        assert!(copied.contains("Text strong with code λ."));
        assert!(!copied.contains("**"));
        assert!(!copied.contains('`'));
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);
    }

    #[test]
    fn markdown_reader_selection_is_per_tab() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs = vec![
            tab_with("a.md", Some("/tmp/a.md"), "alpha\n"),
            tab_with("b.md", Some("/tmp/b.md"), "bravo\n"),
        ];
        app.active_tab = 0;
        app.sync_active_tab();
        app.markdown.read_selection_anchor = Some(1);
        app.markdown.read_selection_cursor = Some(4);
        app.sync_active_tab();

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown.read_selection_range(), None);
        app.markdown.read_selection_anchor = Some(0);
        app.markdown.read_selection_cursor = Some(2);
        app.sync_active_tab();

        app.active_tab = 0;
        app.sync_active_tab();
        assert_eq!(app.markdown.read_selection_range(), Some(1..4));

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown.read_selection_range(), Some(0..2));
    }

    #[test]
    fn markdown_code_copy_hover_lifecycle_clears_on_leave_and_reenters_as_copy() {
        let mut state = MarkdownTabState::default();

        assert!(state.mark_code_copy_hover_valid());
        assert!(state.code_copy_hover_valid);
        assert!(state.clear_code_copy_transient());
        assert!(!state.code_copy_hover_valid);
        assert_eq!(state.copied_code_block, None);

        assert!(state.mark_code_copy_hover_valid());
        state.copied_code_block = Some(10);
        assert!(!state.update_code_copy_hover(Some(10)));
        assert!(state.clear_code_copy_transient());
        assert!(!state.code_copy_hover_valid);
        assert_eq!(state.copied_code_block, None);

        assert!(state.mark_code_copy_hover_valid());
        assert!(!state.update_code_copy_hover(Some(10)));
        assert_eq!(state.copied_code_block, None);
        state.copied_code_block = Some(10);
        assert!(state.update_code_copy_hover(Some(20)));
        assert_eq!(state.copied_code_block, None);
    }

    #[test]
    fn markdown_code_copy_shared_scroll_change_drops_check_without_losing_pointer_validity() {
        let mut state = MarkdownTabState::default();
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        state.code_copy_hover_valid = true;
        state.copied_code_block = Some(10);
        scroll.animate_to(120.0);

        assert!(scroll.update(1.0 / 60.0));
        state.on_shared_vertical_scroll_changed();
        assert_eq!(state.copied_code_block, None);
        assert!(state.code_copy_hover_valid);

        state.copied_code_block = Some(10);
        scroll.animate_to(0.0);
        assert!(scroll.update(1.0 / 60.0));
        state.on_shared_vertical_scroll_changed();
        assert_eq!(state.copied_code_block, None);
        assert!(state.code_copy_hover_valid);
    }

    #[test]
    fn markdown_code_copy_transient_state_does_not_survive_tab_switch() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs = vec![
            tab_with("a.md", Some("/tmp/a.md"), "```rust\na\n```\n"),
            tab_with("b.md", Some("/tmp/b.md"), "```bash\nb\n```\n"),
        ];
        app.active_tab = 0;
        app.sync_active_tab();
        app.markdown.code_copy_hover_valid = true;
        app.markdown.copied_code_block = Some(1);

        app.switch_to_tab(1);
        assert!(!app.markdown.code_copy_hover_valid);
        assert_eq!(app.markdown.copied_code_block, None);

        app.switch_to_tab(0);
        assert!(!app.markdown.code_copy_hover_valid);
        assert_eq!(app.markdown.copied_code_block, None);
    }

    #[test]
    fn markdown_code_copy_does_not_touch_hidden_source_cursor_or_selection() {
        let source = "```rust\nlet x = 1;\n```\n";
        let Some(mut app) = markdown_source_app(source) else {
            return;
        };
        app.editor.cursor = 7;
        app.editor.selection_anchor = Some(2);
        let hidden = (app.editor.cursor, app.editor.selection_anchor);
        app.set_markdown_mode(MarkdownMode::Read);
        app.markdown.read_layout =
            crate::render_view::markdown_read::build_test_markdown_read_layout(source, 500.0);

        assert!(app.copy_markdown_read_code_block(0));
        assert_eq!(app.markdown.copied_code_block, Some(0));
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);
    }

    #[test]
    fn markdown_reader_search_scrolls_read_surface_without_touching_hidden_selection() {
        let mut source = String::from("# top\n\nfirst needle\n\n");
        for i in 0..120 {
            source.push_str("paragraph ");
            source.push_str(&i.to_string());
            source.push_str(" with padding words\n\n");
        }
        source.push_str("last needle\n");
        let Some(mut app) = markdown_source_app(&source) else {
            return;
        };
        app.editor.cursor = 5;
        app.editor.selection_anchor = Some(1);
        let hidden = (app.editor.cursor, app.editor.selection_anchor);
        app.set_markdown_mode(MarkdownMode::Read);
        app.markdown.read_layout =
            crate::render_view::markdown_read::build_test_markdown_read_layout(&source, 320.0);
        app.markdown
            .set_read_scroll_bounds(app.markdown.read_layout.content_height());

        let first = source.find("needle").expect("first match");
        let last = source.rfind("needle").expect("last match");
        app.search_results = vec![
            (first, first + "needle".len()),
            (last, last + "needle".len()),
        ];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
        let first_target = app.scroll_y.target;
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);

        app.search_current_idx = Some(1);
        app.jump_to_search_result();
        let last_target = app.scroll_y.target;
        assert!(last_target > first_target);
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);
    }

    #[test]
    fn markdown_edit_search_still_selects_source_match() {
        let Some(mut app) = markdown_source_app("before needle after\n") else {
            return;
        };
        let start = app.editor.get_full_text().find("needle").expect("match");
        let end = start + "needle".len();
        app.search_results = vec![(start, end)];
        app.search_current_idx = Some(0);

        app.jump_to_search_result();

        assert_eq!(app.editor.cursor, end);
        assert_eq!(app.editor.selection_anchor, Some(start));
    }

    #[test]
    fn markdown_read_mode_is_unavailable_for_special_tabs() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.file_path = Some(PathBuf::from("/tmp/diff.md"));
        app.file_extension = "md".to_string();
        let mut special = tab_with("diff.md", Some("/tmp/diff.md"), "# diff\n");
        special.kind = EditorTabKind::GitDiff(
            crate::app::git_diff::GitDiffTabMeta {
                repo_root: PathBuf::from("/tmp"),
                rel_path: "diff.md".to_string(),
                old_rel_path: None,
                status: crate::app::git_panel::GitFileStatus::Modified,
                workspace_idx: 0,
            },
            crate::app::git_diff::GitDiffState::loading(1),
        );
        app.tabs.push(special);
        app.active_tab = 0;

        app.set_markdown_mode(MarkdownMode::Read);

        assert!(!app.active_document_is_markdown());
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown.mode, MarkdownMode::Edit);
        assert!(app.markdown.read_parser.is_none());
    }
    #[test]
    fn markdown_mode_toggle_ui_click_uses_central_mode_api() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n");

        app.handle_ui_click(crate::ui_system::UiId::MarkdownModeToggle);
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        app.handle_ui_click(crate::ui_system::UiId::MarkdownModeToggle);
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
    }

    fn markdown_source_app(text: &str) -> Option<App> {
        let mut app = test_app()?;
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.base_title = "readme.md".to_string();
        app.editor = editor_with(text);
        Some(app)
    }

    fn workspace_edit_replacing_first_line(
        path: &std::path::Path,
        replacement: &str,
    ) -> crate::lsp::WorkspaceEdit {
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            path.to_path_buf(),
            vec![crate::lsp::TextChange {
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 7,
                new_text: replacement.to_string(),
            }],
        );
        crate::lsp::WorkspaceEdit { changes }
    }

    #[test]
    fn entering_markdown_read_invalidates_source_edit_transients() {
        let Some(mut app) = markdown_source_app("# title\n") else {
            return;
        };
        let path = app.file_path.clone().expect("markdown path");
        app.autocomplete_active = true;
        app.autocomplete_pending_request_id = Some(41);
        app.autocomplete_pending_request_mode = Some(crate::app::AutocompleteMode::LspContext);
        app.autocomplete_pending_request_path = Some(path);
        app.autocomplete_pending_context_key = Some("ctx".to_string());
        app.autocomplete_signature_request_id = Some(42);
        app.autocomplete_detail_request_id = Some(43);
        app.autocomplete_detail_word = Some("title".to_string());
        app.lsp_actions_menu = Some(crate::app::LspActionsMenu {
            cursor_line: 0,
            items: vec![crate::app::LspActionItem::AddNoqaAll],
            selected: 0,
            menu_x: 0.0,
            menu_y: 0.0,
            pending_request_id: Some(44),
        });
        app.pending_fix_all_id = Some(45);

        app.set_markdown_mode(MarkdownMode::Read);

        assert!(!app.autocomplete_active);
        assert_eq!(app.autocomplete_pending_request_id, None);
        assert_eq!(app.autocomplete_pending_request_mode, None);
        assert_eq!(app.autocomplete_pending_request_path, None);
        assert_eq!(app.autocomplete_pending_context_key, None);
        assert_eq!(app.autocomplete_signature_request_id, None);
        assert_eq!(app.autocomplete_detail_request_id, None);
        assert_eq!(app.autocomplete_detail_word, None);
        assert!(app.lsp_actions_menu.is_none());
        assert_eq!(app.pending_fix_all_id, None);

        app.set_markdown_mode(MarkdownMode::Edit);
        assert_eq!(app.pending_fix_all_id, None);
        assert!(app.lsp_actions_menu.is_none());
        assert!(!app.autocomplete_active);
    }

    #[test]
    fn markdown_read_workspace_edit_is_blocked_and_edit_mode_recovers() {
        let Some(mut app) = markdown_source_app("# title\nbody\n") else {
            return;
        };
        let path = app.file_path.clone().expect("markdown path");
        let edit = workspace_edit_replacing_first_line(&path, "# changed");
        app.set_markdown_mode(MarkdownMode::Read);
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.apply_workspace_edit(&edit, true);

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(app.readonly_notice_until.is_some());

        app.set_markdown_mode(MarkdownMode::Edit);
        app.apply_workspace_edit(&edit, true);
        assert_eq!(app.editor.get_full_text(), "# changed\nbody\n");
        assert!(app.editor.version > version);
        assert!(app.editor.is_dirty());
    }

    #[test]
    fn markdown_read_noqa_is_blocked_at_central_mutation_boundary() {
        let Some(mut app) = markdown_source_app("value = 1\n") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.insert_noqa_comment(0, &["F401".to_string()]);

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(app.readonly_notice_until.is_some());
    }

    #[test]
    fn markdown_read_autocomplete_cannot_mutate_hidden_source() {
        let Some(mut app) = markdown_source_app("prin") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        app.autocomplete_active = true;
        app.autocomplete_selected_idx = 0;
        app.autocomplete_options = vec![(
            crate::app::AutocompleteItem {
                word: "print".to_string(),
                kind: crate::highlighter::SymbolKind::Function,
                scope_start: 0,
                scope_end: usize::MAX,
                module: None,
                module_path: None,
                detail: None,
                insert_text: Some("print".to_string()),
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            Vec::new(),
        )];
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.apply_autocomplete();

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(!app.autocomplete_active);
        assert!(app.readonly_notice_until.is_some());
    }

    #[test]
    fn markdown_read_lsp_menu_action_uses_central_readonly_barrier() {
        let Some(mut app) = markdown_source_app("value = 1\n") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        app.lsp_actions_menu = Some(crate::app::LspActionsMenu {
            cursor_line: 0,
            items: vec![crate::app::LspActionItem::AddNoqaAll],
            selected: 0,
            menu_x: 0.0,
            menu_y: 0.0,
            pending_request_id: None,
        });
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.apply_selected_lsp_action();

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(app.lsp_actions_menu.is_none());
        assert!(app.readonly_notice_until.is_some());
    }

    #[test]
    fn markdown_read_lsp_panel_fix_all_is_rejected_before_request_path() {
        let Some(mut app) = markdown_source_app("# title\n") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        app.readonly_notice_until = None;

        app.handle_ui_click(crate::ui_system::UiId::LspServerFixAll(0));

        assert_eq!(app.pending_fix_all_id, None);
        assert!(app.readonly_notice_until.is_some());
    }
