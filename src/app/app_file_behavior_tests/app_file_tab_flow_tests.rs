#[test]
fn focused_file_tree_f2_opens_rename_even_with_stale_api_focus() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-file-tree-f2-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rename_me.py");
    std::fs::write(&path, "print('ok')\n").unwrap();

    app.is_ide_mode = true;
    app.ide_workspaces = vec![dir.clone()];
    app.ide_panel.file_tree_focused = true;
    app.ide_panel.file_tree_selection.insert(path.clone());
    app.ide_panel.api.focused = Some(crate::app::api_client::ApiFocus::RouteFilter);

    assert!(app.handle_file_tree_shortcut(
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F2),
        false,
    ));
    let dialog = app
        .ide_panel
        .file_tree_rename_dialog
        .as_ref()
        .expect("F2 should open rename dialog for the selected file");
    assert_eq!(dialog.path, path);
    assert_eq!(dialog.editor.get_full_text(), "rename_me.py");

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn closing_definition_tab_resets_transient_editor_state() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.editor = editor_with("box.id");
    app.file_path = Some(PathBuf::from("/tmp/main.py"));
    app.file_extension = "py".to_string();
    app.base_title = "main.py".to_string();
    app.scroll_y.current = 300.0;
    app.autocomplete_active = true;
    app.autocomplete_pending_request_id = Some(7);
    app.tabs.push(EditorTab {
        editor: editor_with("box.id"),
        file_path: Some(PathBuf::from("/tmp/main.py")),
        file_key: Some(crate::platform::PathKey::new(std::path::Path::new(
            "/tmp/main.py",
        ))),
        text_file_format: crate::platform::TextFileFormat::default(),
        base_title: "main.py".to_string(),
        file_extension: "py".to_string(),
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: true,
        is_highlight_complete: true,
        icon_key: "python",
        syntax_errors: Vec::new(),
        kind: EditorTabKind::Normal,
    });
    app.tabs[0].scroll_y.current = 300.0;
    app.tabs.push(EditorTab {
        editor: editor_with("class BoxReadPublic:\n    id: int\n"),
        file_path: Some(PathBuf::from("/tmp/output.py")),
        file_key: Some(crate::platform::PathKey::new(std::path::Path::new(
            "/tmp/output.py",
        ))),
        text_file_format: crate::platform::TextFileFormat::default(),
        base_title: "output.py".to_string(),
        file_extension: "py".to_string(),
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: vec![crate::highlighter::ColorSpan {
            start: 0,
            end: 5,
            color: crate::highlighter::DRACULA_PINK,
        }],
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: true,
        is_highlight_complete: true,
        icon_key: "python",
        syntax_errors: Vec::new(),
        kind: EditorTabKind::Normal,
    });
    app.active_tab = 1;
    app.sync_active_tab();

    let old_version = app.tabs[0].editor.version;
    app.close_tab_at(1);

    assert_eq!(app.file_path.as_deref(), Some(Path::new("/tmp/main.py")));
    assert!(app.editor.version > old_version);
    assert!(!app.autocomplete_active);
    assert_eq!(app.autocomplete_pending_request_id, None);
    assert_eq!(app.scroll_y.current, 300.0);
}

#[test]
fn ty_context_exact_single_match_hides_without_waiting_for_response() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("box.model_dump");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(42);
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "model_dump".to_string(),
            kind: SymbolKind::Function,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("BoxRead".to_string()),
            module_path: None,
            detail: Some("def BoxRead.model_dump(self) -> dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    assert!(app.autocomplete_has_only_current_text_match());
    app.hide_autocomplete_popup_keep_request();

    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());
    assert_eq!(app.autocomplete_pending_request_id, Some(42));
    assert_eq!(app.autocomplete_mode, AutocompleteMode::TyContext);
}

#[test]
fn python_import_completion_guard_rejects_def_async_and_strings() {
    let ok = editor_with("Path");
    assert!(python_import_completion_allowed(&ok));

    let mut blank = editor_with("\n");
    blank.cursor = 0;
    assert!(python_import_completion_allowed(&blank));

    let mut in_def = editor_with("def func(");
    in_def.cursor = in_def.len();
    assert!(!python_import_completion_allowed(&in_def));

    let mut in_async = editor_with("async ");
    in_async.cursor = in_async.len();
    assert!(!python_import_completion_allowed(&in_async));

    let mut in_string = editor_with("value = \"Pa");
    in_string.cursor = in_string.len();
    assert!(!python_import_completion_allowed(&in_string));

    let member = editor_with("value.attr");
    assert!(!python_import_completion_allowed(&member));
}

#[test]
fn python_completion_context_detects_member_dot_and_call_parens() {
    let after_dot = editor_with("value.attr");
    assert!(cursor_after_python_member_dot(&after_dot));
    assert!(!cursor_inside_python_call_parens(&after_dot));

    let double_dot = editor_with("box..");
    assert!(!cursor_after_python_member_dot(&double_dot));

    let bare_dot = editor_with(".");
    assert!(!cursor_after_python_member_dot(&bare_dot));

    let in_call = editor_with("call(arg");
    assert!(cursor_inside_python_call_parens(&in_call));
    assert!(!cursor_after_python_member_dot(&in_call));

    let multiline_call = editor_with("asyncpg.create_pool(\n    ma");
    assert!(cursor_inside_python_call_parens(&multiline_call));

    let closed_multiline_call = editor_with("asyncpg.create_pool()\nma");
    assert!(!cursor_inside_python_call_parens(&closed_multiline_call));

    let plain = editor_with("plain");
    assert!(!cursor_after_python_member_dot(&plain));
    assert!(!cursor_inside_python_call_parens(&plain));
}

fn python_editor_at_marker(source: &str) -> Editor {
    let marker = source
        .find('|')
        .expect("test source must contain cursor marker");
    let mut text = source.to_string();
    text.remove(marker);
    let mut editor = editor_with(&text);
    editor.cursor = marker;
    editor
}

#[test]
fn python_completion_guard_rejects_all_string_literal_forms() {
    for source in [
        "'text| remaining'",
        "\"text| remaining\"",
        "r'raw\\|text'",
        "R\"raw|text\"",
        "b'bytes|text'",
        "u'unicode|text'",
        "br'raw bytes|text'",
        "RB\"raw bytes|text\"",
        "'''multi\nline| text'''",
        "\"\"\"multi\nline| text\"\"\"",
        "'escaped \\' quote| text'",
        "\"hash # stays|string\"",
        "# comment| text",
    ] {
        let editor = python_editor_at_marker(source);
        assert!(
            !python_completion_allowed_at_cursor(&editor),
            "completion unexpectedly allowed for {source:?}"
        );
    }

    for source in [
        "'closed'|",
        "\"closed\" + na|me",
        "'''closed'''\nna|me",
        "'unterminated\nna|me",
        "name| # later comment",
    ] {
        let editor = python_editor_at_marker(source);
        assert!(
            python_completion_allowed_at_cursor(&editor),
            "completion unexpectedly blocked for {source:?}"
        );
    }
}

#[test]
fn python_completion_guard_allows_only_f_string_replacement_fields() {
    for source in [
        "f'literal| {value}'",
        "f'{value} literal|'",
        "f'escaped {{literal|}}'",
        "rf'raw literal| {value}'",
        "f'''multi\nliteral| {value}'''",
        "f'{outer:{width}} literal|'",
        "f'{value!r:>10} literal|'",
        "f'{call(\"nested|string\")}'",
    ] {
        let editor = python_editor_at_marker(source);
        assert!(
            !python_completion_allowed_at_cursor(&editor),
            "completion unexpectedly allowed in f-string text for {source:?}"
        );
    }

    for source in [
        "f'{val|ue}'",
        "F\"{obj.at|tr}\"",
        "rf'{call(ar|g)}'",
        "rf'\\{val|ue}'",
        "fr'{mapping[{key|: value}]}'",
        "f'{outer:{wid|th}}'",
        "f'{\"nested\" + val|ue}'",
        "f'''multi\n{obj.at|tr}\nline'''",
    ] {
        let editor = python_editor_at_marker(source);
        assert!(
            python_completion_allowed_at_cursor(&editor),
            "completion unexpectedly blocked in replacement field for {source:?}"
        );
    }

    let member_literal = python_editor_at_marker("f'obj.at|tr'");
    assert!(!cursor_after_python_member_dot(&member_literal));
    let member_field = python_editor_at_marker("f'{obj.at|tr}'");
    assert!(cursor_after_python_member_dot(&member_field));

    let call_literal = python_editor_at_marker("f'call(ar|g)'");
    assert!(!cursor_inside_python_call_parens(&call_literal));
    let call_field = python_editor_at_marker("f'{call(ar|g)}'");
    assert!(cursor_inside_python_call_parens(&call_field));
}

#[test]
fn python_tree_sitter_completion_closes_in_string_and_opens_in_f_string_field() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.highlighter.completions = vec![completion("print", SymbolKind::Function, 0, usize::MAX)];
    app.editor = python_editor_at_marker("message = 'pri|'");
    app.autocomplete_active = true;

    app.update_autocomplete();

    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());

    app.editor = python_editor_at_marker("message = f'{pri|}'");
    app.update_autocomplete();

    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_options[0].0.word, "print");
}

#[test]
fn python_completion_closes_for_implausibly_deep_member_chain() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("box.ooo.o.o.o.o.o");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;

    assert!(python_member_chain_too_deep(&app.editor));
    app.update_autocomplete();

    assert!(!app.autocomplete_active);
}

#[test]
fn tab_sync_swaps_editor_metadata_and_current_icon() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("live");
    app.base_title = "*live.rs".to_string();
    app.file_path = Some(PathBuf::from("/tmp/live.rs"));
    app.file_extension = "rs".to_string();
    app.search_results = vec![(0, 1)];
    app.search_current_idx = Some(0);
    app.last_sent_version = 7;
    app.is_highlighted_once = true;
    app.is_highlight_complete = true;

    app.tabs
        .push(tab_with("other.py", Some("/tmp/other.py"), "tab text"));
    app.active_tab = 0;

    app.sync_active_tab();

    assert_eq!(app.editor.get_full_text(), "tab text");
    assert_eq!(app.base_title, "other.py");
    assert_eq!(app.file_extension, "py");
    assert_eq!(app.tabs[0].editor.get_full_text(), "live");
    assert_eq!(app.tabs[0].base_title, "*live.rs");
    assert_eq!(app.tabs[0].search_results, vec![(0, 1)]);
    assert_eq!(app.tabs[0].search_current_idx, Some(0));
    assert_eq!(app.tabs[0].last_sent_version, 7);
    assert!(app.tabs[0].is_highlighted_once);
    assert!(app.tabs[0].is_highlight_complete);
    assert_eq!(
        app.tabs[0].icon_key,
        crate::app::file_icons::file_icon_key("live.rs")
    );
}

#[test]
fn close_current_file_resets_editor_search_scroll_and_welcome_state() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("dirty text");
    app.file_path = Some(PathBuf::from("/tmp/current.py"));
    app.base_title = "current.py".to_string();
    app.file_extension = "py".to_string();
    app.search_results = vec![(0, 5)];
    app.search_current_idx = Some(0);
    app.show_search = true;
    app.autocomplete_active = true;
    app.show_welcome = false;
    app.scroll_y.current = 123.0;
    app.scroll_y.target = 456.0;
    app.scroll_x.current = 12.0;
    app.scroll_x.target = 34.0;

    app.close_current_file();

    assert_eq!(app.base_title, "Добро пожаловать");
    assert!(app.file_path.is_none());
    assert_eq!(app.file_extension, "");
    assert_eq!(app.editor.get_full_text(), "");
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_current_idx, None);
    assert!(!app.show_search);
    assert!(!app.autocomplete_active);
    assert!(app.show_welcome);
    assert_eq!(app.scroll_y.current, 0.0);
    assert_eq!(app.scroll_x.target, 0.0);
}

#[test]
fn file_loading_saving_and_missing_file_cleanup_update_state_without_window() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-app-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("demo.py");
    std::fs::write(&path, "print('hi')\n").unwrap();

    app.load_file_internal(path.clone(), false, false);
    let canonical_path = crate::platform::canonicalize_or_absolutize(&path);
    assert_eq!(app.file_path.as_ref(), Some(&canonical_path));
    assert_eq!(app.base_title, "demo.py");
    assert_eq!(app.file_extension, "py");
    assert_eq!(app.editor.get_full_text(), "print('hi')\n");
    assert!(!app.show_welcome);
    assert_eq!(app.scroll_y.current, 0.0);
    assert_eq!(app.last_sent_version, u64::MAX);

    app.editor = editor_with("print('bye')\n");
    app.file_path = Some(path.clone());
    assert!(app.save_current_file());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "print('bye')\n");
    assert!(!app.editor.is_dirty());

    let missing = dir.join("missing.py");
    app.recent_files = vec![missing.clone(), path.clone()];
    app.load_file_internal(missing.clone(), false, false);
    assert_eq!(app.recent_files, vec![canonical_path]);

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn file_open_and_atomic_save_preserve_encoding_bom_and_line_endings() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-text-format-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let formats = [
        (
            "utf8-bom.txt",
            crate::platform::TextFileFormat {
                encoding: crate::platform::TextEncoding::Utf8Bom,
                line_ending: crate::platform::LineEnding::CrLf,
            },
        ),
        (
            "utf16-le.txt",
            crate::platform::TextFileFormat {
                encoding: crate::platform::TextEncoding::Utf16Le,
                line_ending: crate::platform::LineEnding::CrLf,
            },
        ),
        (
            "utf16-be.txt",
            crate::platform::TextFileFormat {
                encoding: crate::platform::TextEncoding::Utf16Be,
                line_ending: crate::platform::LineEnding::Cr,
            },
        ),
    ];

    for (name, format) in formats {
        let path = dir.join(name);
        std::fs::write(
            &path,
            crate::platform::encode_text("first\n😀second\n", format),
        )
        .unwrap();

        app.load_file_internal(path.clone(), false, false);
        assert_eq!(app.editor.get_full_text(), "first\n😀second\n");
        assert_eq!(app.text_file_format, format);
        assert_eq!(app.file_key, Some(crate::platform::PathKey::new(&path)));

        app.editor = editor_with("first\n😀changed\n");
        app.file_path = Some(path.clone());
        app.file_key = Some(crate::platform::PathKey::new(&path));
        assert!(app.save_current_file());

        let decoded = crate::platform::read_text_file(&path).unwrap();
        assert_eq!(decoded.text, "first\n😀changed\n");
        assert_eq!(decoded.format, format);
    }

    let leftovers = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains("rriter-tmp"))
        .count();
    assert_eq!(leftovers, 0);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn save_as_changes_document_identity_only_after_successful_atomic_write() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-save-as-transaction-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let old_path = dir.join("old.txt");
    std::fs::write(&old_path, "old\n").unwrap();
    app.file_path = Some(old_path.clone());
    app.file_key = Some(crate::platform::PathKey::new(&old_path));
    app.base_title = "old.txt".to_string();
    app.file_extension = "txt".to_string();
    app.text_file_format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf16Le,
        line_ending: crate::platform::LineEnding::CrLf,
    };
    app.editor = editor_with("new\n😀value\n");

    let missing_target = dir.join("missing").join("new.md");
    assert!(!app.save_current_file_as(missing_target.clone()));
    assert_eq!(app.file_path.as_deref(), Some(old_path.as_path()));
    assert_eq!(app.file_key, Some(crate::platform::PathKey::new(&old_path)));
    assert_eq!(app.base_title, "old.txt");
    assert_eq!(app.file_extension, "txt");
    assert!(!missing_target.exists());

    let target = dir.join("new.md");
    assert!(app.save_current_file_as(target.clone()));
    let target = crate::platform::canonicalize_or_absolutize(&target);
    assert_eq!(app.file_path.as_deref(), Some(target.as_path()));
    assert_eq!(app.file_key, Some(crate::platform::PathKey::new(&target)));
    assert_eq!(app.base_title, "new.md");
    assert_eq!(app.file_extension, "md");
    assert!(!app.editor.is_dirty());
    assert_eq!(app.recent_files.first(), Some(&target));
    let decoded = crate::platform::read_text_file(&target).unwrap();
    assert_eq!(decoded.text, "new\n😀value\n");
    assert_eq!(decoded.format, app.text_file_format);

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn autosave_only_runs_in_ide_mode() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-autosave-mode-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("note.txt");
    std::fs::write(&path, "old\n").unwrap();

    app.is_ide_mode = false;
    app.file_path = Some(path.clone());
    app.text_file_format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf8,
        line_ending: crate::platform::LineEnding::Lf,
    };
    app.editor = editor_with("old\n");
    app.editor.insert_str("dirty\n");
    assert!(app.editor.is_dirty());
    assert!(!app.autosave_current_file_if_dirty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");

    app.is_ide_mode = true;
    assert!(app.autosave_current_file_if_dirty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\ndirty\n");

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn internal_editor_focus_loss_autosaves_dirty_file() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-autosave-focus-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("focus.py");
    std::fs::write(&path, "old\n").unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(path.clone());
    app.file_extension = "py".to_string();
    app.text_file_format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf8,
        line_ending: crate::platform::LineEnding::Lf,
    };
    app.editor = editor_with("old\n");
    app.editor.insert_str("changed\n");
    assert!(app.editor_has_input_focus());
    assert!(app.editor.is_dirty());

    let editor_was_focused = app.editor_has_input_focus();
    app.ide_panel.terminal_focused = true;
    assert!(app.autosave_after_editor_focus_change(editor_was_focused));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\nchanged\n");
    assert!(!app.editor.is_dirty());

    app.editor.insert_str("again\n");
    assert!(!app.autosave_after_editor_focus_change(false));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\nchanged\n");

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn file_open_waits_for_tree_sitter_and_applies_folds_before_return() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-open-highlight-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("folds.py");
    std::fs::write(&path, "import os\nimport sys\n\nprint('ready')\n").unwrap();
    app.highlighter
        .reset(1, "warmup = 1\n".to_string(), "py".to_string(), 0);
    assert!(
        app.highlighter
            .wait_for_first_result(1, std::time::Duration::from_secs(2))
    );
    app.editor.version = 1;

    app.load_file_internal(path.clone(), false, true);

    assert!(app.is_highlighted_once);
    assert!(!app.highlighter.spans.is_empty());
    assert!(!app.editor.foldable_ranges_bytes.is_empty());
    assert!(!app.editor.folded_lines.is_empty());

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn large_rust_file_open_waits_for_priority_highlight_before_return() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-large-rust-open-highlight-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("large.rs");
    let text = "fn panel_row() {\n    let value = 1;\n}\n".repeat(2400);
    assert!(text.len() > crate::highlighter::TREE_SITTER_HIGHLIGHT_MAX_BYTES);
    std::fs::write(&path, text).unwrap();

    app.load_file_internal(path.clone(), false, true);

    assert!(app.is_highlighted_once);
    assert!(app.highlighter.spans.iter().any(|span| span.start == 0));

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn same_version_problem_jump_reprioritize_waits_for_target_highlight() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-problem-repriority-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("large.rs");
    let prefix = "fn filler() {\n    let value = 1;\n}\n".repeat(900);
    let target = "fn target_problem() {\n    let value = 2;\n}\n";
    let suffix = "fn after() {\n    let value = 3;\n}\n".repeat(1800);
    let target_offset = prefix.len() + "fn ".len();
    let text = format!("{prefix}{target}{suffix}");
    assert!(text.len() > crate::highlighter::TREE_SITTER_HIGHLIGHT_MAX_BYTES);
    std::fs::write(&path, text).unwrap();

    app.load_file_internal(path.clone(), false, true);
    assert!(app.is_highlighted_once);
    assert_eq!(app.highlighter.current_version, app.editor.version);

    app.editor.cursor = target_offset;
    app.reprioritize_highlighter_around_cursor();
    assert!(app.highlighter.current_version < app.editor.version);
    app.wait_for_current_highlight();

    assert!(app.is_highlighted_once);
    assert_eq!(app.highlighter.current_version, app.editor.version);
    assert!(app.highlighter.spans.iter().any(|span| {
        span.start <= target_offset && span.end >= target_offset + "target_problem".len()
    }));

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn file_open_prefolds_imports_without_waiting_for_highlighter() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-open-prefold-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("folds.py");
    std::fs::write(&path, "import os\nimport sys\n\nprint('ready')\n").unwrap();

    app.load_file_internal(path.clone(), false, false);

    assert!(!app.is_highlighted_once);
    assert!(!app.editor.foldable_ranges_bytes.is_empty());
    assert!(app.editor.folded_lines.contains(&0));
    assert!(app.editor.folded_start_bytes.contains(&0));

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn file_open_prefolds_python_bracket_blocks_without_waiting_for_highlighter() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-open-bracket-prefold-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("folds.py");
    std::fs::write(
            &path,
            "handlers = [\n    AuthController,\n]\n\nexception_handlers={\n    Exception: handler,\n},  # ty\n",
        )
        .unwrap();

    app.load_file_internal(path.clone(), false, false);

    assert!(!app.is_highlighted_once);
    assert!(app.editor.folded_lines.contains(&0));
    assert!(app.editor.folded_lines.contains(&4));

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn late_highlight_does_not_autofold_extra_blocks_after_prefolded_open() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("import os\nimport sys\n\ndef later():\n    return 1\n");
    app.file_extension = "py".to_string();
    let text = app.editor.get_full_text();
    apply_initial_import_folds(&mut app.editor, &app.file_extension, &text);
    assert!(app.editor.folded_lines.contains(&0));

    let fn_start = text.find("def later").unwrap();
    let fn_end = text.rfind("return 1").unwrap() + "return 1".len();
    app.highlighter.foldable_ranges = vec![(fn_start, fn_end, true, false)];

    app.apply_highlight_results();

    assert!(app.is_highlighted_once);
    assert!(!app.editor.folded_lines.contains(&3));
}

#[test]
fn highlight_results_update_fold_maps_and_autofold_once() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("fn main() {\n  if true {\n    println!(\"x\");\n  }\n}\n");
    app.file_extension = "rs".to_string();
    let text = app.editor.get_full_text();
    let block_start = text.find("fn main").unwrap();
    let block_end = text.rfind('}').unwrap();
    app.highlighter.foldable_ranges = vec![(block_start, block_end, true, true)];

    app.apply_highlight_results();

    assert!(app.is_highlighted_once);
    assert_eq!(app.editor.foldable_ranges_bytes.len(), 1);
    assert!(app.editor.foldable_lines.contains_key(&0));
    assert!(app.editor.folded_lines.contains(&0));
    assert!(app.editor.folded_start_bytes.contains(&0));

    app.editor.folded_lines.clear();
    app.highlighter.foldable_ranges = vec![(block_start, block_end, true, false)];
    app.apply_highlight_results();
    assert!(app.editor.folded_lines.is_empty());
}

#[test]
fn highlight_wait_falls_back_to_bounded_sync_parse_after_worker_timeout() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("def ready():\n    return True\n");
    app.is_highlighted_once = false;
    app.is_highlight_complete = false;
    while app.highlighter.rx.try_recv().is_ok() {}
    app.reset_highlighter_with_text(app.editor.get_full_text(), false);

    app.wait_for_current_highlight_with_timeouts(
        std::time::Duration::ZERO,
        std::time::Duration::from_secs(1),
    );

    assert!(app.is_highlighted_once);
    assert!(app.is_highlight_complete);
    assert_eq!(app.highlighter.current_version, app.editor.version);
    assert!(!app.highlighter.spans.is_empty());
}

#[test]
fn check_external_changes_refreshes_clean_tabs_and_leaves_dirty_tabs_alone() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-tabs-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let clean_path = dir.join("clean.py");
    let dirty_path = dir.join("dirty.txt");
    std::fs::write(&clean_path, "def clean():\n    return 1\n").unwrap();
    std::fs::write(&dirty_path, "disk dirty\n").unwrap();

    let mut clean_tab = tab_with(
        "clean.py",
        Some(clean_path.to_str().unwrap()),
        "def clean():\n    return 0\n",
    );
    clean_tab.editor.set_original_text();
    let mut dirty_tab = tab_with(
        "dirty.txt",
        Some(dirty_path.to_str().unwrap()),
        "old dirty\n",
    );
    let _ = dirty_tab.editor.insert_str("local change");
    app.tabs = vec![clean_tab, dirty_tab];
    app.active_tab = 0;
    app.editor = Editor::new(32);
    app.base_title = "scratch".to_string();
    app.sync_active_tab();

    app.check_external_changes();
    app.sync_active_tab();

    let clean = app
        .tabs
        .iter()
        .find(|tab| tab.file_path.as_ref() == Some(&clean_path))
        .unwrap();
    let dirty = app
        .tabs
        .iter()
        .find(|tab| tab.file_path.as_ref() == Some(&dirty_path))
        .unwrap();
    assert_eq!(clean.editor.get_full_text(), "def clean():\n    return 1\n");
    assert!(clean.is_highlighted_once);
    assert!(dirty.editor.get_full_text().contains("local change"));

    std::fs::remove_file(clean_path).ok();
    std::fs::remove_file(dirty_path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn async_external_changes_reload_clean_tabs_without_blocking_highlight_wait() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-tabs-async-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let clean_path = dir.join("clean.py");
    std::fs::write(&clean_path, "def clean():\n    return 2\n").unwrap();

    let mut clean_tab = tab_with(
        "clean.py",
        Some(clean_path.to_str().unwrap()),
        "def clean():\n    return 1\n",
    );
    clean_tab.editor.set_original_text();
    app.tabs = vec![clean_tab];
    app.active_tab = 0;
    app.editor = Editor::new(32);
    app.base_title = "scratch".to_string();
    app.sync_active_tab();

    app.start_external_changes_check();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline && !app.poll_external_changes() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert_eq!(app.editor.get_full_text(), "def clean():\n    return 2\n");
    assert!(!app.is_highlighted_once);

    std::fs::remove_file(clean_path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn app_tabs_recent_files_search_jump_and_autocomplete_empty_paths() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.open_new_tab();
    assert!(app.tabs.is_empty());
    assert!(app.show_welcome);
    assert_eq!(app.base_title, "Добро пожаловать");

    app.is_ide_mode = true;
    app.open_new_tab();
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.base_title, "Безымянный");
    assert!(!app.show_welcome);

    app.editor = editor_with("first tab");
    app.base_title = "first.py".to_string();
    app.file_extension = "py".to_string();
    app.file_path = Some(PathBuf::from("/tmp/first.py"));
    app.open_new_tab();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.editor.get_full_text(), "");
    assert_eq!(app.base_title, "Безымянный");

    app.close_tab_at(0);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);

    for idx in 0..12 {
        app.add_recent_file(PathBuf::from(format!("/tmp/recent-{idx}.py")));
    }
    app.add_recent_file(PathBuf::from("/tmp/recent-5.py"));
    assert_eq!(app.recent_files.len(), 10);
    assert_eq!(app.recent_files[0], PathBuf::from("/tmp/recent-5.py"));
    assert_eq!(
        app.recent_files
            .iter()
            .filter(|p| **p == PathBuf::from("/tmp/recent-5.py"))
            .count(),
        1
    );

    app.editor = editor_with("one two one");
    app.search_editor = editor_with("one");
    app.update_search();
    assert_eq!(app.search_results, vec![(0, 3), (8, 11)]);
    app.search_current_idx = Some(1);
    app.jump_to_search_result();
    assert_eq!(app.editor.selection_anchor, Some(8));
    assert_eq!(app.editor.cursor, 11);

    app.editor = editor_with("pri.");
    app.editor.cursor = 4;
    assert_eq!(app.get_current_word_prefix(), "");
    app.update_autocomplete();
    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());

    app.editor = editor_with("pr");
    app.highlighter.completions = vec![
        completion("print", SymbolKind::Function, 0, 10),
        completion("private", SymbolKind::Variable, 1, 100),
        completion("property", SymbolKind::Class, 0, 100),
        completion("pr", SymbolKind::Variable, 0, 100),
    ];
    app.update_autocomplete();
    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_options.len(), 3);
    assert_eq!(app.autocomplete_options[0].0.word, "private");
    assert_eq!(app.autocomplete_options[1].0.word, "print");
    assert_eq!(app.autocomplete_options[2].0.word, "property");
}

#[test]
fn closing_tab_clamps_stale_tab_scroll_left() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.tabs = vec![
        tab_with("first.py", Some("/tmp/first.py"), "first"),
        tab_with("second.py", Some("/tmp/second.py"), "second"),
    ];
    app.active_tab = 0;
    app.sync_active_tab();
    app.tab_scroll.current = 500.0;
    app.tab_scroll.target = 500.0;

    app.close_tab_at(1);

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tab_scroll.current, 0.0);
    assert_eq!(app.tab_scroll.target, 0.0);
}

fn file_tab_test_diag(message: &str) -> crate::lsp::Diagnostic {
    crate::lsp::Diagnostic {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 1,
        severity: crate::lsp::DiagSeverity::Error,
        code: None,
        code_href: None,
        message: std::sync::Arc::<str>::from(message),
        source: Some(std::sync::Arc::<str>::from("ty")),
        quickfixes: Vec::new().into_boxed_slice(),
        tags: Vec::new().into_boxed_slice(),
    }
}

#[test]
fn closing_last_tab_from_workspace_removes_diagnostics_from_all_problems_source() {
    let Some(mut app) = test_app() else {
        return;
    };
    let ws_a = PathBuf::from("/tmp/rriter-ws-a");
    let ws_b = PathBuf::from("/tmp/rriter-ws-b");
    let a_file = ws_a.join("a.py");
    let b_file = ws_b.join("b.py");
    let stale = ws_b.join("pkg/stale.py");
    let keep = ws_a.join("pkg/keep.py");
    app.is_ide_mode = true;
    app.ide_workspaces = vec![ws_a.clone(), ws_b.clone()];
    app.tabs = vec![
        tab_with("a.py", Some(a_file.to_str().unwrap()), "a = 1\n"),
        tab_with("b.py", Some(b_file.to_str().unwrap()), "b = 1\n"),
    ];
    app.active_tab = 0;
    app.editor = Editor::new(32);
    app.sync_active_tab();

    let mut lsp = crate::lsp::LspManager::new(vec![ws_a.clone(), ws_b.clone()]);
    lsp.python_disabled = true;
    lsp.notify_open(&a_file, "py", "a = 1\n", 1);
    lsp.notify_open(&b_file, "py", "b = 1\n", 1);
    lsp.diagnostics
        .insert(stale.clone(), vec![file_tab_test_diag("stale")].into());
    lsp.diagnostics
        .insert(keep.clone(), vec![file_tab_test_diag("keep")].into());
    app.lsp = Some(lsp);

    app.close_tab_at(1);

    let lsp = app.lsp.as_ref().unwrap();
    assert!(!lsp.diagnostics.contains_key(&stale));
    assert_eq!(lsp.get_diagnostics(&keep).len(), 1);
}

#[test]
fn closing_single_python_file_uses_old_extension_and_clears_lsp_diagnostics() {
    let Some(mut app) = test_app() else {
        return;
    };
    let ws = PathBuf::from("/tmp/rriter-ws-single");
    let path = ws.join("main.py");
    app.is_ide_mode = true;
    app.ide_workspaces = vec![ws.clone()];
    app.tabs = vec![tab_with(
        "main.py",
        Some(path.to_str().unwrap()),
        "print(1)\n",
    )];
    app.active_tab = 0;
    app.editor = Editor::new(32);
    app.sync_active_tab();

    let mut lsp = crate::lsp::LspManager::new(vec![ws]);
    lsp.python_disabled = true;
    lsp.notify_open(&path, "py", "print(1)\n", 1);
    lsp.diagnostics
        .insert(path.clone(), vec![file_tab_test_diag("stale")].into());
    app.lsp = Some(lsp);

    app.close_current_file();

    let lsp = app.lsp.as_ref().unwrap();
    assert!(lsp.diagnostics.is_empty());
    assert!(app.tabs.is_empty());
    assert_eq!(app.file_extension, "");
}

#[test]
fn tab_context_menu_targets_clicked_tab_with_path_actions() {
    let Some(mut app) = test_app() else {
        return;
    };
    let workspace = PathBuf::from("/tmp/rriter-tab-context-workspace");
    let active_path = workspace.join("src/active.rs");
    let inactive_path = workspace.join("tests/inactive.rs");
    app.is_ide_mode = true;
    app.ide_workspaces = vec![workspace];
    app.tabs = vec![
        tab_with("active.rs", Some("/tmp/stale-active.rs"), "active"),
        tab_with(
            "inactive.rs",
            Some(inactive_path.to_str().unwrap()),
            "inactive",
        ),
    ];
    app.active_tab = 0;
    app.file_path = Some(active_path.clone());
    assert!(app.open_tab_context_menu(0, 120.0, 48.0));
    let menu = app.ide_panel.file_tree_context_menu.as_ref().unwrap();
    assert_eq!(menu.x, 130.0);
    assert_eq!(menu.y, 58.0);
    assert!(!app.popup_blocks_background_at(menu.x, menu.y));
    assert_eq!(menu.target_path.as_ref(), Some(&active_path));
    assert_eq!(menu.target_dir.as_deref(), active_path.parent());
    assert_eq!(
        menu.entries,
        vec![
            crate::app::file_tree::FileTreeMenuAction::ShowInExplorer,
            crate::app::file_tree::FileTreeMenuAction::OpenContainedFolder,
            crate::app::file_tree::FileTreeMenuAction::CopyTargetAbsolutePath,
            crate::app::file_tree::FileTreeMenuAction::CopyTargetRelativePath,
        ]
    );

    assert!(app.open_tab_context_menu_for_hit(
        crate::ui_system::UiId::EditorTabClose(1),
        320.0,
        48.0,
    ));
    let menu = app.ide_panel.file_tree_context_menu.as_ref().unwrap();
    assert_eq!(menu.target_path.as_ref(), Some(&inactive_path));
    assert!(!app.open_tab_context_menu_for_hit(
        crate::ui_system::UiId::WelcomeNewFile,
        0.0,
        0.0,
    ));
    assert!(!app.open_tab_context_menu(2, 0.0, 0.0));
}

#[test]
fn show_tab_path_in_explorer_opens_expands_selects_and_centers_file() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-tab-reveal-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let workspace = std::env::temp_dir().join(unique);
    let src_dir = workspace.join("src");
    let path = src_dir.join("main.rs");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(&path, "fn main() {}\n").unwrap();

    app.is_ide_mode = true;
    app.ide_workspaces = vec![workspace.clone()];
    app.ide_panel.open(crate::app::PanelId::Search);
    app.ide_panel.file_tree_nodes = vec![crate::app::file_tree::FileNode {
        path: path.clone(),
        name: "main.rs".to_string(),
        depth: 2,
        is_dir: false,
        is_expanded: false,
        icon_key: "rust",
        is_ignored: false,
    }];
    app.ide_panel.explorer_scroll.current = 90.0;
    app.ide_panel.explorer_scroll.target = 90.0;
    let menu = crate::app::file_tree::FileTreeContextMenu {
        x: 0.0,
        y: 0.0,
        target_path: Some(path.clone()),
        target_is_dir: false,
        target_dir: Some(src_dir.clone()),
        entries: vec![crate::app::file_tree::FileTreeMenuAction::ShowInExplorer],
        opened_at: Instant::now(),
    };

    app.handle_file_tree_menu_action(
        crate::app::file_tree::FileTreeMenuAction::ShowInExplorer,
        menu,
    );

    assert!(app.ide_panel.is_open(crate::app::PanelId::Explorer));
    assert!(!app.ide_panel.is_open(crate::app::PanelId::Search));
    assert!(app.ide_panel.file_tree_focused);
    assert_eq!(app.ide_panel.file_tree_selection.len(), 1);
    assert!(app.ide_panel.file_tree_selection.contains(&path));
    assert!(app.ide_panel.file_tree_expanded.contains(&workspace));
    assert!(app.ide_panel.file_tree_expanded.contains(&src_dir));
    assert_eq!(app.ide_panel.explorer_scroll.current, 0.0);
    assert_eq!(app.ide_panel.explorer_scroll.target, 0.0);

    drop(app);
    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn pending_action_save_existing_file_runs_only_after_successful_write() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-pending-save-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("saved.txt");
    std::fs::write(&path, "old").unwrap();

    app.editor = editor_with("new contents");
    let _ = app.editor.insert_str("!");
    app.file_path = Some(path.clone());
    app.pending_action = PendingAction::CloseFile;
    app.pending_action_ready = false;

    app.begin_pending_action_save();

    assert!(app.pending_action_ready);
    assert!(!app.pending_action_waiting_for_save_as);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new contents!");
    assert!(!app.editor.is_dirty());

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pending_action_save_failure_does_not_execute_action() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-pending-save-failure-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir()
        .join(unique)
        .join("missing-parent")
        .join("saved.txt");

    app.editor = editor_with("new contents");
    let _ = app.editor.insert_str("!");
    app.file_path = Some(path);
    app.pending_action = PendingAction::CloseFile;
    app.pending_action_ready = false;

    app.begin_pending_action_save();

    assert!(!app.pending_action_ready);
    assert!(app.editor.is_dirty());
    assert!(app.ide_panel.file_tree_error.is_some());
}

#[test]
fn save_as_selection_resumes_pending_action_only_after_success() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-pending-save-as-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("chosen.txt");

    app.editor = editor_with("chosen contents");
    let _ = app.editor.insert_str("!");
    app.file_path = None;
    app.pending_action = PendingAction::CloseFile;
    app.pending_action_waiting_for_save_as = true;
    app.pending_action_ready = false;

    assert!(app.handle_save_as_selection(Some(path.clone())));
    assert!(!app.pending_action_waiting_for_save_as);
    assert!(app.pending_action_ready);
    assert_eq!(app.file_path.as_deref(), Some(path.as_path()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "chosen contents!");

    app.pending_action_waiting_for_save_as = true;
    app.pending_action_ready = false;
    assert!(!app.handle_save_as_selection(None));
    assert!(!app.pending_action_waiting_for_save_as);
    assert!(!app.pending_action_ready);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dirty_text_tab_requires_confirmation_before_close() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.tabs = vec![
        tab_with("first.py", Some("/tmp/first.py"), "first"),
        tab_with("second.py", Some("/tmp/second.py"), "second"),
    ];
    app.active_tab = 0;
    app.sync_active_tab();
    let _ = app.tabs[1].editor.insert_str(" changed");

    app.close_tab_at(1);

    assert_eq!(app.tabs.len(), 2);
    assert!(matches!(app.pending_action, PendingAction::CloseTab(1)));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn dirty_single_text_tab_requires_confirmation_for_legacy_close_index() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs = vec![tab_with("scratch.py", None, "scratch")];
    app.active_tab = 0;
    app.sync_active_tab();
    let _ = app.editor.insert_str(" changed");

    app.close_tab_at(usize::MAX);

    assert_eq!(app.tabs.len(), 1);
    assert!(matches!(app.pending_action, PendingAction::CloseTab(0)));
    assert!(!app.show_welcome);
}

#[test]
fn unsaved_change_detection_includes_inactive_tabs() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.tabs = vec![
        tab_with("clean.py", Some("/tmp/clean.py"), "clean"),
        tab_with("dirty.py", Some("/tmp/dirty.py"), "dirty"),
    ];
    app.active_tab = 0;
    app.sync_active_tab();
    let _ = app.tabs[1].editor.insert_str(" changed");

    assert!(app.has_unsaved_changes());
}

#[test]
fn pending_quit_save_writes_every_dirty_text_tab() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-pending-save-tabs-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.txt");
    let second = dir.join("second.txt");
    std::fs::write(&first, "first").unwrap();
    std::fs::write(&second, "second").unwrap();

    app.is_ide_mode = true;
    app.tabs = vec![
        tab_with("first.txt", first.to_str(), "first"),
        tab_with("second.txt", second.to_str(), "second"),
    ];
    app.active_tab = 0;
    app.sync_active_tab();
    let _ = app.editor.insert_str(" active");
    let _ = app.tabs[1].editor.insert_str(" inactive");
    app.pending_action = PendingAction::Quit;

    app.begin_pending_action_save();

    assert!(app.pending_action_ready);
    assert!(app.pending_save_tabs.is_empty());
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "first active");
    assert_eq!(
        std::fs::read_to_string(&second).unwrap(),
        "second inactive"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
