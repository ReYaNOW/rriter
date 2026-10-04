#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AutocompleteKeyAction, autocomplete_key_action, autocomplete_next_index};

    #[test]
    fn pdf_editor_filter_uses_commands_and_rejects_editor_conflicts() {
        let platform = crate::platform::CURRENT_PLATFORM;
        let mut overrides = crate::keymap::KeymapOverrides::default();
        let save_on_open = crate::keymap::Chord::parse(platform, "mod+o").expect("valid chord");
        overrides.add_chord(platform, crate::keymap::Command::FileSave, save_on_open);
        let keymap = crate::keymap::Keymap::build_for(platform, &overrides);
        assert!(!pdf_tab_key_reaches_editor(Some(save_on_open), &keymap));

        let find_on_g = crate::keymap::Chord::parse(platform, "mod+g").expect("valid chord");
        overrides.add_chord(platform, crate::keymap::Command::SearchEditorOpen, find_on_g);
        let keymap = crate::keymap::Keymap::build_for(platform, &overrides);
        assert!(pdf_tab_key_reaches_editor(Some(find_on_g), &keymap));

        overrides.add_chord(platform, crate::keymap::Command::FileSave, find_on_g);
        let keymap = crate::keymap::Keymap::build_for(platform, &overrides);
        assert!(!pdf_tab_key_reaches_editor(Some(find_on_g), &keymap));
    }

    #[test]
    fn autocomplete_key_action_maps_navigation_and_apply_keys() {
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::Escape)),
            AutocompleteKeyAction::DismissAndConsume
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowLeft)),
            AutocompleteKeyAction::DismissAndContinue
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowRight)),
            AutocompleteKeyAction::DismissAndContinue
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowDown)),
            AutocompleteKeyAction::MoveDown
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowUp)),
            AutocompleteKeyAction::MoveUp
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::Enter)),
            AutocompleteKeyAction::Apply
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::Tab)),
            AutocompleteKeyAction::Apply
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::KeyA)),
            AutocompleteKeyAction::None
        );
    }

    #[test]
    fn autocomplete_ctrl_navigation_jumps_five_items() {
        assert_eq!(autocomplete_next_index(0, 10, false, false), 1);
        assert_eq!(autocomplete_next_index(0, 10, false, true), 5);
        assert_eq!(autocomplete_next_index(2, 10, true, false), 1);
        assert_eq!(autocomplete_next_index(7, 10, true, true), 2);
        assert_eq!(autocomplete_next_index(0, 0, false, true), 0);
    }

    #[test]
    fn autocomplete_ctrl_navigation_stops_at_edge_before_wrapping() {
        assert_eq!(autocomplete_next_index(2, 10, true, true), 0);
        assert_eq!(autocomplete_next_index(3, 10, true, true), 0);
        assert_eq!(autocomplete_next_index(4, 10, true, true), 0);
        assert_eq!(autocomplete_next_index(0, 10, true, true), 9);

        assert_eq!(autocomplete_next_index(5, 10, false, true), 9);
        assert_eq!(autocomplete_next_index(6, 10, false, true), 9);
        assert_eq!(autocomplete_next_index(7, 10, false, true), 9);
        assert_eq!(autocomplete_next_index(9, 10, false, true), 0);
    }

    #[test]
    fn backspace_cross_line_detects_only_line_changes() {
        assert!(!backspace_crossed_line(&[0, 4, 8], 5, &[0, 4, 7], 4));
        assert!(backspace_crossed_line(&[0, 4, 8], 4, &[0, 7], 3));
    }

    #[test]
    fn sync_edit_line_range_covers_insert_delete_empty_and_last_line() {
        let lines = vec![0, 6, 12, 20];

        assert_eq!(sync_edit_line_range(&[], &lines, 24), (None, None));

        let edits = vec![crate::highlighter::SyncEdit::Insert {
            offset: 7,
            text: "abc".to_string(),
        }];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(6), Some(12))
        );

        let edits = vec![crate::highlighter::SyncEdit::Delete { offset: 18, len: 3 }];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(12), Some(20))
        );

        let edits = vec![
            crate::highlighter::SyncEdit::Insert {
                offset: 2,
                text: "x".to_string(),
            },
            crate::highlighter::SyncEdit::Delete { offset: 18, len: 2 },
        ];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(0), Some(20))
        );

        let edits = vec![
            crate::highlighter::SyncEdit::Insert {
                offset: 7,
                text: " ".to_string(),
            },
            crate::highlighter::SyncEdit::Delete { offset: 7, len: 1 },
        ];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(6), Some(12))
        );

        let edits = vec![crate::highlighter::SyncEdit::Insert {
            offset: 3,
            text: "x".to_string(),
        }];
        assert_eq!(sync_edit_line_range(&edits, &[], 9), (Some(0), Some(9)));
    }

    #[test]
    fn bounded_repeat_scroll_delta_ignores_stale_large_focus_jump() {
        assert_eq!(bounded_repeat_scroll_delta(22.0, 12.0), Some(22.0));
        assert_eq!(bounded_repeat_scroll_delta(-25.0, 12.0), None);
    }

    #[test]
    fn period_key_falls_back_to_dot_text_on_press() {
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), None, None, false),
            Some(".")
        );
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), Some("."), None, false),
            Some(".")
        );
        assert_eq!(
            key_text_for_editor_insert(
                PhysicalKey::Code(KeyCode::NumpadDecimal),
                None,
                None,
                false
            ),
            Some(".")
        );
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), None, Some(">"), true),
            Some(">")
        );
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), None, None, true),
            None
        );
    }

    #[test]
    fn editor_insert_text_rejects_ascii_control_characters() {
        assert_eq!(key_text_for_editor_insert(PhysicalKey::Code(KeyCode::KeyA), Some("\u{8}"), None, false), None);
        assert_eq!(key_text_for_editor_insert(PhysicalKey::Code(KeyCode::KeyA), Some("\u{7f}"), None, false), None);
        assert_eq!(key_text_for_editor_insert(PhysicalKey::Code(KeyCode::KeyA), Some("a"), None, false), Some("a"));
        assert_eq!(key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Tab), Some("\t"), None, false), Some("\t"));
    }

    #[test]
    fn paired_editor_insert_text_reuses_file_editor_pairs() {
        assert_eq!(paired_editor_insert_text("("), ("()", true));
        assert_eq!(paired_editor_insert_text("["), ("[]", true));
        assert_eq!(paired_editor_insert_text("{"), ("{}", true));
        assert_eq!(paired_editor_insert_text("'"), ("''", true));
        assert_eq!(paired_editor_insert_text("\""), ("\"\"", true));
        assert_eq!(paired_editor_insert_text("x"), ("x", false));
    }

    #[test]
    fn line_comment_shortcut_uses_primary_modifier_and_normalized_language() {
        for (extension, expected) in [
            ("py", Some("#")),
            ("pyi", Some("#")),
            ("sh", Some("#")),
            ("jsx", Some("//")),
            ("hpp", Some("//")),
            ("sql", Some("--")),
            ("json", None),
            ("md", None),
            ("txt", None),
        ] {
            assert_eq!(
                editor_line_comment_marker(extension, PhysicalKey::Code(KeyCode::Slash), true,),
                expected,
                "extension {extension}"
            );
        }
        assert_eq!(
            editor_line_comment_marker("rs", PhysicalKey::Code(KeyCode::Slash), false),
            None
        );
        assert_eq!(
            editor_line_comment_marker("rs", PhysicalKey::Code(KeyCode::KeyC), true),
            None
        );
        assert_eq!(
            key_text_for_editor_insert(
                PhysicalKey::Code(KeyCode::Slash),
                Some("/"),
                Some("/"),
                false,
            ),
            Some("/")
        );

        let mut unsupported = crate::editor::Editor::new(32);
        unsupported.set_clean_text("value\n");
        unsupported.cursor = 2;
        let unsupported_before = unsupported.get_full_text();
        let unsupported_version = unsupported.version;
        let unsupported_history = unsupported.history.len();
        if let Some(marker) = editor_line_comment_marker(
            "json",
            PhysicalKey::Code(KeyCode::Slash),
            true,
        ) {
            unsupported.toggle_line_comment(marker);
        }
        assert_eq!(unsupported.get_full_text(), unsupported_before);
        assert_eq!(unsupported.version, unsupported_version);
        assert_eq!(unsupported.history.len(), unsupported_history);

        let mut sql = crate::editor::Editor::new(32);
        sql.set_clean_text("select 1;\n");
        let marker = editor_line_comment_marker(
            "sql",
            PhysicalKey::Code(KeyCode::Slash),
            true,
        )
        .expect("SQL line marker");
        assert!(sql.toggle_line_comment(marker));
        assert_eq!(sql.get_full_text(), "--select 1;\n");
    }

    #[test]
    fn ctrl_slash_comment_toggle_keeps_highlighter_sync_replica_current() {
        let source = "fn main() {}\n";
        let mut editor = crate::editor::Editor::new(64);
        editor.set_clean_text(source);

        let mut highlighter = crate::highlighter::Highlighter::new();
        highlighter.reset(1, source.to_string(), "rs".to_string(), 0);
        assert!(highlighter.wait_for_first_result(1, std::time::Duration::from_secs(2)));

        let marker = editor_line_comment_marker("rs", PhysicalKey::Code(KeyCode::Slash), true)
            .expect("Rust line marker");
        assert!(toggle_editor_line_comment_and_sync_highlighter(
            &mut editor,
            &mut highlighter,
            marker,
        ));
        let edits = std::mem::take(&mut editor.sync_edits);
        let (line_start_byte, line_end_byte) =
            sync_edit_line_range(&edits, &editor.line_offsets, editor.len());
        let (invalidate_start_byte, invalidate_end_byte) =
            crate::highlighter::sync_edit_invalidation_byte_range(&edits);
        highlighter.apply_edits(editor.version, edits, line_start_byte, line_end_byte);
        assert!(highlighter.sync_highlight_after_edit(
            editor.version,
            line_start_byte,
            line_end_byte,
            invalidate_start_byte,
            invalidate_end_byte,
            std::time::Duration::from_millis(50),
        ));

        assert_eq!(editor.get_full_text(), "//fn main() {}\n");
        assert!(highlighter.spans.iter().any(|span| {
            span.start == 0 && span.end >= 2 && span.role == crate::theme::SyntaxRole::Comment
        }));
    }

    #[test]
    fn markdown_read_primary_slash_is_ignored_without_readonly_notice() {
        assert_eq!(
            markdown_editor_key_action(
                true,
                true,
                PhysicalKey::Code(KeyCode::Slash),
                true,
                false,
                false,
            ),
            None
        );
    }

    #[test]
    fn markdown_read_primary_c_routes_to_reader_selection_copy() {
        let key = PhysicalKey::Code(KeyCode::KeyC);
        assert_eq!(
            markdown_editor_key_action(true, true, key, true, false, false),
            Some(MarkdownEditorKeyAction::CopySelection)
        );
        assert_eq!(
            markdown_editor_key_action(true, false, key, true, false, false),
            None
        );
        assert_eq!(
            markdown_editor_key_action(false, true, key, true, false, false),
            None
        );
    }

    #[test]
    fn markdown_read_consumes_every_editor_mutation_key() {
        for (key, primary, alt, has_text) in [
            (KeyCode::Backspace, false, false, false),
            (KeyCode::Delete, false, false, false),
            (KeyCode::Enter, false, false, false),
            (KeyCode::NumpadEnter, false, false, false),
            (KeyCode::Tab, false, false, false),
            (KeyCode::Space, false, false, false),
            (KeyCode::KeyX, true, false, false),
            (KeyCode::KeyV, true, false, false),
            (KeyCode::KeyZ, true, false, false),
            (KeyCode::KeyY, true, false, false),
            (KeyCode::KeyA, false, false, true),
            (KeyCode::Enter, false, true, false),
        ] {
            assert_eq!(
                markdown_editor_key_action(
                    true,
                    true,
                    PhysicalKey::Code(key),
                    primary,
                    alt,
                    has_text,
                ),
                Some(MarkdownEditorKeyAction::ReadonlyNotice),
                "key {key:?}"
            );
        }
    }

    #[test]
    fn markdown_read_navigation_never_routes_to_source_cursor() {
        assert_eq!(
            markdown_editor_key_action(
                true,
                true,
                PhysicalKey::Code(KeyCode::ArrowUp),
                false,
                false,
                false
            ),
            Some(MarkdownEditorKeyAction::ScrollLines(-1))
        );
        assert_eq!(
            markdown_editor_key_action(
                true,
                true,
                PhysicalKey::Code(KeyCode::PageDown),
                false,
                false,
                false
            ),
            Some(MarkdownEditorKeyAction::ScrollPages(1))
        );
        assert_eq!(
            markdown_editor_key_action(
                true,
                true,
                PhysicalKey::Code(KeyCode::Home),
                false,
                false,
                false
            ),
            Some(MarkdownEditorKeyAction::ScrollStart)
        );
        assert_eq!(
            markdown_editor_key_action(
                true,
                true,
                PhysicalKey::Code(KeyCode::End),
                false,
                false,
                false
            ),
            Some(MarkdownEditorKeyAction::ScrollEnd)
        );
        assert_eq!(
            markdown_editor_key_action(
                true,
                true,
                PhysicalKey::Code(KeyCode::ArrowLeft),
                false,
                false,
                false
            ),
            Some(MarkdownEditorKeyAction::Consume)
        );
    }

    #[test]
    fn markdown_read_ime_commit_is_readonly_and_edit_routing_recovers() {
        use crate::app::app_behavior_tests::{editor_with, test_app};
        use std::path::PathBuf;

        let Some(mut app) = test_app() else {
            return;
        };
        app.show_welcome = false;
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("source");
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        let before = app.editor.get_full_text();
        let version = app.editor.version;
        app.handle_editor_ime_commit("ж");
        assert_eq!(app.editor.get_full_text(), before);
        assert_eq!(app.editor.version, version);
        assert!(app.readonly_notice_until.is_some());

        app.toggle_markdown_mode();
        assert_eq!(app.markdown_mode(), crate::app::MarkdownMode::Edit);
        assert_eq!(
            markdown_editor_key_action(
                true,
                false,
                PhysicalKey::Code(KeyCode::KeyA),
                false,
                false,
                true
            ),
            None
        );
    }
}
