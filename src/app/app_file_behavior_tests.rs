use super::*;

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
        icon_key: "python",
        syntax_errors: Vec::new(),
        kind: EditorTabKind::Normal,
    });
    app.tabs[0].scroll_y.current = 300.0;
    app.tabs.push(EditorTab {
        editor: editor_with("class BoxReadPublic:\n    id: int\n"),
        file_path: Some(PathBuf::from("/tmp/output.py")),
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
    let mut ok = editor_with("\n");
    ok.cursor = 0;
    assert!(python_import_completion_allowed(&ok));

    let mut in_def = editor_with("def func(");
    in_def.cursor = in_def.len();
    assert!(!python_import_completion_allowed(&in_def));

    let mut in_async = editor_with("async ");
    in_async.cursor = in_async.len();
    assert!(!python_import_completion_allowed(&in_async));

    let mut in_string = editor_with("value = \"Pa");
    in_string.cursor = in_string.len();
    assert!(!python_import_completion_allowed(&in_string));
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

    let plain = editor_with("plain");
    assert!(!cursor_after_python_member_dot(&plain));
    assert!(!cursor_inside_python_call_parens(&plain));
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
    app.base_title = "live.rs".to_string();
    app.file_path = Some(PathBuf::from("/tmp/live.rs"));
    app.file_extension = "rs".to_string();
    app.search_results = vec![(0, 1)];
    app.search_current_idx = Some(0);
    app.last_sent_version = 7;
    app.is_highlighted_once = true;

    app.tabs
        .push(tab_with("other.py", Some("/tmp/other.py"), "tab text"));
    app.active_tab = 0;

    app.sync_active_tab();

    assert_eq!(app.editor.get_full_text(), "tab text");
    assert_eq!(app.base_title, "other.py");
    assert_eq!(app.file_extension, "py");
    assert_eq!(app.tabs[0].editor.get_full_text(), "live");
    assert_eq!(app.tabs[0].base_title, "live.rs");
    assert_eq!(app.tabs[0].search_results, vec![(0, 1)]);
    assert_eq!(app.tabs[0].search_current_idx, Some(0));
    assert_eq!(app.tabs[0].last_sent_version, 7);
    assert!(app.tabs[0].is_highlighted_once);
    assert_ne!(app.tabs[0].icon_key, "default_file");
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
    assert_eq!(app.file_path.as_ref(), Some(&path));
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
    assert_eq!(app.recent_files, vec![path.clone()]);

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
    assert!(!clean.spans.is_empty());
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
fn ide_mode_startup_tab_and_tab_close_paths_are_headless_safe() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.show_welcome = true;
    app.base_title = "Добро пожаловать".to_string();
    app.editor = editor_with("startup buffer");
    app.file_extension = "txt".to_string();

    app.enter_ide_mode();
    assert!(app.is_ide_mode);
    assert!(!app.show_welcome);
    assert_eq!(app.base_title, "Безымянный");
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert!(app.lsp.is_some());
    assert!(app.tabs[0].file_path.is_none());

    app.editor = editor_with("active");
    app.base_title = "active.rs".to_string();
    app.file_extension = "rs".to_string();
    app.tabs
        .push(tab_with("other.py", Some("/tmp/other.py"), "other"));
    app.active_tab = 0;

    app.close_tab_at(0);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.editor.get_full_text(), "other");
    assert_eq!(app.base_title, "other.py");

    app.close_tab_at(99);
    assert!(app.show_welcome);
    assert_eq!(app.base_title, "Добро пожаловать");
}

#[test]
fn open_file_in_tab_reuses_existing_tabs_and_loads_into_empty_slot() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-open-tab-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.py");
    let second = dir.join("second.txt");
    std::fs::write(&first, "first\n").unwrap();
    std::fs::write(&second, "second\n").unwrap();

    app.is_ide_mode = true;
    app.tabs.push(tab_with(
        "first.py",
        Some(first.to_str().unwrap()),
        "cached first\n",
    ));
    app.tabs.push(tab_with("scratch", None, ""));
    app.active_tab = 1;
    app.editor = Editor::new(32);
    app.base_title = "scratch".to_string();

    app.open_file_in_tab_bg(first.clone(), false);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.editor.get_full_text(), "");

    app.open_file_in_tab(first.clone(), false);
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.editor.get_full_text(), "cached first\n");

    app.switch_to_tab(1);
    app.open_file_in_tab(second.clone(), false);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.file_path.as_ref(), Some(&second));
    assert_eq!(app.editor.get_full_text(), "second\n");

    std::fs::remove_file(first).ok();
    std::fs::remove_file(second).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn ide_file_opens_do_not_update_recent_files_but_non_ide_opens_do() {
    let Some(mut app) = test_app() else {
        return;
    };
    let unique = format!(
        "rriter-recent-ide-open-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let ide_path = dir.join("ide.py");
    let plain_path = dir.join("plain.py");
    std::fs::write(&ide_path, "print('ide')\n").unwrap();
    std::fs::write(&plain_path, "print('plain')\n").unwrap();

    app.is_ide_mode = true;
    app.open_file_in_tab(ide_path.clone(), true);
    assert!(app.recent_files.is_empty());

    app.is_ide_mode = false;
    app.open_file_in_tab(plain_path.clone(), true);
    assert_eq!(app.recent_files, vec![plain_path.clone()]);

    std::fs::remove_file(ide_path).ok();
    std::fs::remove_file(plain_path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn switch_to_tab_waits_for_highlight_before_return() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.tabs.push(tab_with(
        "first.py",
        Some("/tmp/first.py"),
        "print('first')\n",
    ));
    app.tabs.push(tab_with(
        "second.py",
        Some("/tmp/second.py"),
        "from os import path\nprint(path)\n",
    ));
    app.active_tab = 0;
    app.editor = editor_with("print('first')\n");
    app.file_path = Some(PathBuf::from("/tmp/first.py"));
    app.file_extension = "py".to_string();
    app.base_title = "first.py".to_string();

    app.switch_to_tab(1);

    assert_eq!(
        app.file_path.as_deref(),
        Some(std::path::Path::new("/tmp/second.py"))
    );
    if !app.is_highlighted_once
        && app
            .highlighter
            .wait_for_first_result(app.editor.version, std::time::Duration::from_secs(2))
    {
        app.apply_highlight_results();
    }
    assert!(app.is_highlighted_once);
    assert_eq!(app.highlighter.current_version, app.editor.version);
}

#[test]
fn close_active_tab_uses_version_newer_than_removed_tab_highlighter_watermark() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs
        .push(tab_with("first.py", Some("/tmp/first.py"), "print(1)\n"));
    app.tabs
        .push(tab_with("second.py", Some("/tmp/second.py"), "print(2)\n"));
    app.editor = editor_with("print(2)\n");
    app.file_path = Some(PathBuf::from("/tmp/second.py"));
    app.file_extension = "py".to_string();
    app.base_title = "second.py".to_string();
    app.active_tab = 1;
    app.highlighter.current_version = 500;

    app.close_tab_at(1);

    assert_eq!(app.file_path.as_deref(), Some(Path::new("/tmp/first.py")));
    assert!(app.editor.version > 500);
    if app.highlighter.current_version != app.editor.version
        && app
            .highlighter
            .wait_for_first_result(app.editor.version, std::time::Duration::from_secs(2))
    {
        app.apply_highlight_results();
    }
    assert_eq!(app.highlighter.current_version, app.editor.version);
    assert!(app.is_highlighted_once);
}

#[test]
fn python_assignment_declaration_jump_prefers_nearest_usage() {
    let editor = editor_with("value = build()\nprint(value)\nvalue_other = value\n");
    let source_start = editor.get_full_text().find("value").unwrap();
    let usage = nearest_python_assignment_usage(&editor, (source_start, source_start + 5))
        .expect("expected usage");

    assert_eq!(editor.get_full_text().get(usage..usage + 5), Some("value"));
    assert_eq!(editor.get_full_text()[..usage].lines().count(), 2);
}

#[test]
fn python_assignment_declaration_ignores_def_and_comparisons() {
    let def_editor = editor_with("def value():\n    return value\n");
    let def_start = def_editor.get_full_text().find("value").unwrap();
    assert_eq!(
        nearest_python_assignment_usage(&def_editor, (def_start, def_start + 5)),
        None
    );

    let cmp_editor = editor_with("value == other\nprint(value)\n");
    let cmp_start = cmp_editor.get_full_text().find("value").unwrap();
    assert_eq!(
        nearest_python_assignment_usage(&cmp_editor, (cmp_start, cmp_start + 5)),
        None
    );
}

#[test]
fn python_assignment_declaration_ignores_annotation_type_tokens() {
    let editor = editor_with("title: t.Optional[str] = None\nbody: t.Optional[str] = None\n");
    let optional_start = editor.get_full_text().find("Optional").unwrap();
    let title_start = editor.get_full_text().find("title").unwrap();

    assert_eq!(
        nearest_python_assignment_usage(&editor, (optional_start, optional_start + 8)),
        None
    );
    assert_eq!(
        nearest_python_assignment_usage(&editor, (title_start, title_start + 5)),
        None
    );
}

#[test]
fn ctrl_definition_same_declaration_target_redirects_to_usage() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.file_extension = "py".to_string();
    app.file_path = Some(PathBuf::from("/tmp/ctrl_def.py"));
    app.editor = editor_with("value = build()\nprint(value)\n");

    let source_start = app.editor.get_full_text().find("value").unwrap();
    let source_range = (source_start, source_start + 5);
    let (line, col) = crate::lsp::offset_to_lsp_pos(
        &app.editor.get_full_text(),
        source_start,
        &app.editor.line_offsets,
    );
    app.ctrl_definition.source_path = app.current_abs_path();
    app.ctrl_definition.source_range = Some(source_range);

    let target = app
        .ctrl_definition_target_from_lsp(Some(DefinitionJumpTarget {
            path: PathBuf::from("/tmp/ctrl_def.py"),
            line,
            col,
        }))
        .expect("expected usage target");

    assert_eq!(target.path, PathBuf::from("/tmp/ctrl_def.py"));
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 6);
}

#[test]
fn highlight_thresholds_and_prefix_edges_cover_non_default_paths() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.editor = editor_with("root:\n  a: 1\n  b: 2\n");
    app.file_extension = "yaml".to_string();
    let end = app.editor.len();
    app.highlighter.foldable_ranges = vec![(0, end, true, false)];
    app.apply_highlight_results();
    assert!(app.editor.foldable_lines.contains_key(&0));
    assert!(app.editor.folded_lines.is_empty());

    app.editor = editor_with("obj.attr\nsnake_case");
    app.editor.cursor = app.editor.len();
    assert_eq!(app.get_current_word_prefix(), "snake_case");
    app.editor.cursor = 3;
    assert_eq!(app.get_current_word_prefix(), "obj");
}

#[test]
fn lsp_actions_noqa_workspace_edit_and_panel_log_sizes_headless() {
    let Some(mut app) = test_app() else {
        return;
    };

    let path = PathBuf::from("/tmp/main.py");
    app.file_path = Some(path.clone());
    app.file_extension = "py".to_string();
    app.base_title = "main.py".to_string();
    app.editor = editor_with("x = 1\nvalue = 2  # noqa: E501\n");

    app.insert_noqa_comment(0, &["F401".to_string(), "E501".to_string()]);
    assert!(
        app.editor
            .get_full_text()
            .starts_with("x = 1  # noqa: F401, E501\n")
    );

    app.insert_noqa_comment(1, &["F821".to_string(), "E501".to_string()]);
    assert!(
        app.editor
            .get_full_text()
            .contains("value = 2  # noqa: E501, F821")
    );

    app.insert_noqa_comment(1, &[]);
    assert!(app.editor.get_full_text().contains("value = 2  # noqa\n"));

    app.editor = editor_with("abc\ndef\nghi\n");
    app.editor.cursor = 5;
    app.editor.selection_anchor = Some(1);

    let mut changes = std::collections::HashMap::new();
    changes.insert(
        path.clone(),
        vec![
            crate::lsp::TextChange {
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 3,
                new_text: "DEF".to_string(),
            },
            crate::lsp::TextChange {
                start_line: 0,
                start_col: 1,
                end_line: 0,
                end_col: 2,
                new_text: "B".to_string(),
            },
        ],
    );
    app.apply_workspace_edit(&crate::lsp::WorkspaceEdit { changes }, true);
    assert_eq!(app.editor.get_full_text(), "aBc\nDEF\nghi\n");
    assert_eq!(app.editor.cursor, 5);
    assert_eq!(app.editor.selection_anchor, Some(1));

    let mut action_changes = std::collections::HashMap::new();
    action_changes.insert(
        path.clone(),
        vec![crate::lsp::TextChange {
            start_line: 2,
            start_col: 0,
            end_line: 2,
            end_col: 3,
            new_text: "GHI".to_string(),
        }],
    );
    app.lsp_actions_menu = Some(LspActionsMenu {
        cursor_line: 0,
        items: vec![LspActionItem::CodeAction(crate::lsp::CodeAction {
            title: "Upper".to_string(),
            kind: Some("quickfix".to_string()),
            edit: Some(crate::lsp::WorkspaceEdit {
                changes: action_changes,
            }),
            code: Some("T001".to_string()),
        })],
        selected: 0,
        menu_x: 0.0,
        menu_y: 0.0,
        pending_request_id: None,
    });
    app.apply_selected_lsp_action();
    assert_eq!(app.editor.get_full_text(), "aBc\nDEF\nGHI\n");

    app.lsp_actions_menu = Some(LspActionsMenu {
        cursor_line: 0,
        items: vec![LspActionItem::AddNoqa {
            codes: vec!["T002".to_string()],
        }],
        selected: 0,
        menu_x: 0.0,
        menu_y: 0.0,
        pending_request_id: None,
    });
    app.apply_selected_lsp_action();
    assert!(
        app.editor
            .get_full_text()
            .starts_with("aBc  # noqa: T002\n")
    );

    assert!(app.lsp_panel_bounds().is_none());

    let info = crate::lsp::LspServerInfo {
        name: "ruff",
        status: crate::lsp::LspServerStatus::Running,
        logs: Vec::new(),
    };
    app.ide_panel.lsp_servers = vec![info.clone()];
    assert_eq!(app.lsp_server_logs_h(&info, 1.0), 0.0);

    app.ide_panel.lsp_logs_expanded.insert("ruff".to_string());
    let mut log_editor = editor_with("header\n  detail\nlast line\n");
    log_editor.foldable_lines.insert(0, 1);
    log_editor.folded_lines.insert(0);
    app.ide_panel
        .lsp_log_editors
        .insert("ruff".to_string(), log_editor);

    let (inner_h, inner_w) = app.lsp_server_inner_size(&info, 1.0);
    assert!(inner_h >= 32.0);
    assert!(inner_w > 0.0);
    assert!(app.lsp_server_logs_h(&info, 1.0) >= 50.0);
    assert!(app.lsp_panel_total_h(1.0) >= 210.0);
}

#[test]
fn ui_handlers_state_only_branches_work_without_window() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.handle_ui_click(crate::ui_system::UiId::HoverPopupScroll);
    app.handle_ui_click(crate::ui_system::UiId::BottomPanelBody);
    app.handle_ui_click(crate::ui_system::UiId::StatusDiagnostics);
    assert!(app.ide_panel.is_open(crate::app::PanelId::Problems));
    app.handle_ui_click(crate::ui_system::UiId::StatusDiagnostics);
    assert!(!app.ide_panel.is_open(crate::app::PanelId::Problems));

    app.ide_panel.terminal_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::ResizeLeft);
    app.handle_ui_click(crate::ui_system::UiId::ResizeBottom);
    assert!(!app.ide_panel.is_resizing_left);
    assert!(!app.ide_panel.is_resizing_bottom);

    app.handle_ui_click(crate::ui_system::UiId::LspScrollY);
    app.handle_ui_click(crate::ui_system::UiId::LspScrollX);
    assert!(app.ide_panel.lsp_scroll_y.is_dragging);
    assert!(app.ide_panel.lsp_scroll_x.is_dragging);

    app.handle_ui_click(crate::ui_system::UiId::EditorScrollbarX);
    assert!(app.scroll_x.is_dragging);

    app.ide_panel.lsp_servers = vec![crate::lsp::LspServerInfo {
        name: "ruff",
        status: crate::lsp::LspServerStatus::Running,
        logs: Vec::new(),
    }];
    app.handle_ui_click(crate::ui_system::UiId::LspLogScrollY(0));
    app.handle_ui_click(crate::ui_system::UiId::LspLogScrollX(0));
    assert!(
        app.ide_panel
            .lsp_logs_scroll_y
            .get("ruff")
            .is_some_and(|scroll| scroll.is_dragging)
    );
    assert!(
        app.ide_panel
            .lsp_logs_scroll_x
            .get("ruff")
            .is_some_and(|scroll| scroll.is_dragging)
    );

    app.ide_panel
        .flat_diags
        .push((PathBuf::from("/tmp/main.py"), 0));
    app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
    assert!(app.ide_panel.problems_collapsed.is_empty());
}

#[test]
fn git_panel_ui_handlers_cover_menu_commit_and_folder_state_headless() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: Vec::new(),
            tree: vec![crate::app::git_panel::GitTreeRow {
                name: "src".to_string(),
                path: "src".to_string(),
                depth: 0,
                file_idx: None,
                icon_key: "src",
            }],
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuToggle);
    assert!(app.ide_panel.git.commit_menu_open);

    app.handle_ui_click(crate::ui_system::UiId::GitFolder(0, 0));
    assert!(!app.ide_panel.git.commit_menu_open);
    assert!(
        app.ide_panel
            .git
            .collapsed_dirs
            .get(&0)
            .is_some_and(|dirs| dirs.contains("src"))
    );

    app.handle_ui_click(crate::ui_system::UiId::GitFolder(0, 0));
    assert!(
        !app.ide_panel
            .git
            .collapsed_dirs
            .get(&0)
            .is_some_and(|dirs| dirs.contains("src"))
    );

    app.ide_panel.git.message_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::GitCommit);
    assert_eq!(
        app.ide_panel.git.notice.as_deref(),
        Some("Commit message empty")
    );
    assert!(!app.ide_panel.git.message_focused);

    let _ = app.ide_panel.git.message_editor.insert_str("ready");
    app.ide_panel.git.message_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuItem(1));
    assert_eq!(app.ide_panel.git.notice.as_deref(), Some("No staged files"));
    assert!(!app.ide_panel.git.message_focused);
}

#[test]
fn git_panel_workspace_confirm_dialogs_use_staged_files_headless() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![
                crate::app::git_panel::GitFileEntry {
                    workspace_idx: 0,
                    repo_root: PathBuf::from("/workspace"),
                    rel_path: "src/lib.rs".to_string(),
                    old_rel_path: None,
                    display_path: "src/lib.rs".to_string(),
                    depth: 1,
                    staged: true,
                    status: crate::app::git_panel::GitFileStatus::Modified,
                },
                crate::app::git_panel::GitFileEntry {
                    workspace_idx: 0,
                    repo_root: PathBuf::from("/workspace"),
                    rel_path: "src/main.rs".to_string(),
                    old_rel_path: None,
                    display_path: "src/main.rs".to_string(),
                    depth: 1,
                    staged: false,
                    status: crate::app::git_panel::GitFileStatus::Modified,
                },
            ],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitRollbackStaged(0));
    let dialog = app.ide_panel.git.confirm_dialog.as_ref().unwrap();
    assert_eq!(
        dialog.action,
        crate::app::git_panel::GitConfirmAction::RollbackStaged
    );
    assert_eq!(dialog.files.len(), 1);
    assert_eq!(dialog.files[0].display_path, "src/lib.rs");

    app.handle_ui_click(crate::ui_system::UiId::GitConfirmCancel);
    assert!(app.ide_panel.git.confirm_dialog.is_none());

    app.handle_ui_click(crate::ui_system::UiId::GitUnstageAll(0));
    assert!(app.ide_panel.git.confirm_dialog.is_none());
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
}

#[test]
fn git_panel_stage_clicks_are_locked_while_pending_headless() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                repo_root: PathBuf::from("/workspace"),
                rel_path: "tests/test_api.py".to_string(),
                old_rel_path: None,
                display_path: "tests/test_api.py".to_string(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: vec![
                crate::app::git_panel::GitTreeRow {
                    name: "tests".to_string(),
                    path: "tests".to_string(),
                    depth: 0,
                    file_idx: None,
                    icon_key: "tests",
                },
                crate::app::git_panel::GitTreeRow {
                    name: "test_api.py".to_string(),
                    path: "tests/test_api.py".to_string(),
                    depth: 1,
                    file_idx: Some(0),
                    icon_key: "python",
                },
            ],
            ahead: 0,
            error: None,
        }],
    };
    app.ide_panel.git.pending = true;

    app.handle_ui_click(crate::ui_system::UiId::GitFolderStage(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.ide_panel.git.stage_pending_workspace_idx, None);

    app.handle_ui_click(crate::ui_system::UiId::GitFile(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
}

#[test]
fn git_file_row_checkbox_only_toggles_stage() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                display_path: "src/main.rs".to_string(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitFile(0, 0));
    assert!(app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert!(app.tabs.is_empty());
}

#[test]
fn git_file_label_double_click_opens_diff_not_stage() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                display_path: "src/main.rs".to_string(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitFileDiff(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.ide_panel.git.selected_file, Some((0, 0)));
    assert!(app.tabs.is_empty());

    app.handle_ui_click(crate::ui_system::UiId::GitFileDiff(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.base_title, "Diff: main.rs");
    assert!(matches!(
        &app.tabs[0].kind,
        crate::app::EditorTabKind::GitDiff(_, _)
    ));
}

#[test]
fn diff_tab_dedup_by_repo_path() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                display_path: "src/main.rs".to_string(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.open_git_diff_tab(0, 0);
    app.open_git_diff_tab(0, 0);
    assert_eq!(app.tabs.len(), 1);
}

#[test]
fn undo_redo_rebuilds_diff_decorations() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    let mut state =
        crate::app::git_diff::build_diff_view("a\nold\n".to_string(), "a\nnew\n".to_string());
    state.version = 1;
    let text = state.displayed_text.clone();
    app.editor = editor_with(&text);
    app.editor.set_original_text();
    app.base_title = "Diff: main.rs".to_string();
    app.tabs.push(EditorTab {
        editor: editor_with(&text),
        file_path: None,
        base_title: "Diff: main.rs".to_string(),
        file_extension: "rs".to_string(),
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: true,
        icon_key: "default_file",
        syntax_errors: Vec::new(),
        kind: crate::app::EditorTabKind::GitDiff(
            crate::app::git_diff::GitDiffTabMeta {
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                status: crate::app::git_panel::GitFileStatus::Modified,
                workspace_idx: 0,
            },
            state,
        ),
    });

    app.rollback_active_git_diff_hunk(0);
    assert!(app.active_git_diff_state().unwrap().hunks.is_empty());
    app.editor.undo();
    app.rebuild_active_git_diff_from_editor_after_history(true);
    assert_eq!(app.active_git_diff_state().unwrap().hunks.len(), 1);
    app.editor.redo();
    app.rebuild_active_git_diff_from_editor_after_history(false);
    assert!(app.active_git_diff_state().unwrap().hunks.is_empty());
}

#[test]
fn inline_git_rollback_undo_restores_cursor_to_changed_block_end() {
    let Some(mut app) = test_app() else {
        return;
    };
    let base = "a\nold\nz\n";
    let changed = "a\nnew\nz\n";
    app.editor = editor_with(changed);
    app.editor.cursor = 0;
    app.editor.set_git_base_text(Some(base.to_string()));
    assert_eq!(app.editor.git_hunks.len(), 1);
    app.highlighter.spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: 1,
        color: crate::highlighter::DRACULA_FG,
    }];
    app.inline_git_popup = Some(crate::app::InlineGitPopup {
        hunk_idx: 0,
        anchor_line: 1,
        lines: Vec::new(),
        spans: Vec::new(),
        diff_state: crate::app::git_diff::build_diff_view(base.to_string(), changed.to_string()),
    });

    app.rollback_inline_git_hunk();

    assert_eq!(app.editor.get_full_text(), base);
    assert!(!app.highlighter.spans.is_empty());
    app.editor.undo();
    assert_eq!(app.editor.get_full_text(), changed);
    assert_eq!(app.editor.cursor, "a\nnew\n".len());
}

#[test]
fn ui_handlers_search_problem_log_and_diagnostic_actions_are_headless_safe() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.editor = editor_with("alpha beta alpha");
    app.editor.cursor = 0;
    app.search_editor = editor_with("alpha");
    app.show_search = true;
    app.search_focused = true;
    app.update_search();
    assert_eq!(app.search_current_idx, Some(0));

    app.handle_ui_click(crate::ui_system::UiId::SearchNext);
    assert_eq!(app.search_current_idx, Some(1));
    assert_eq!(app.editor.selection_anchor, Some(11));
    assert_eq!(app.editor.cursor, 16);

    app.handle_ui_click(crate::ui_system::UiId::SearchPrev);
    assert_eq!(app.search_current_idx, Some(0));
    assert_eq!(app.editor.selection_anchor, Some(0));
    assert_eq!(app.editor.cursor, 5);

    app.handle_ui_click(crate::ui_system::UiId::SearchCaseToggle);
    assert!(app.search_case_sensitive);
    app.handle_ui_click(crate::ui_system::UiId::SearchClose);
    assert!(!app.show_search);
    assert!(!app.search_focused);
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_current_idx, None);

    let path = PathBuf::from("/tmp/main.py");
    app.ide_panel.flat_diags.push((path.clone(), usize::MAX));
    app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
    assert!(app.ide_panel.problems_collapsed.contains(&path));
    app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
    assert!(!app.ide_panel.problems_collapsed.contains(&path));

    app.handle_ui_click(crate::ui_system::UiId::ProblemsTab(2));
    assert_eq!(app.ide_panel.problems_tab, 2);

    app.ide_panel.lsp_servers = vec![crate::lsp::LspServerInfo {
        name: "ruff",
        status: crate::lsp::LspServerStatus::Running,
        logs: Vec::new(),
    }];
    let mut log_editor = editor_with("line one\nline two\n");
    log_editor.selection_anchor = Some(0);
    app.ide_panel
        .lsp_log_editors
        .insert("ruff".to_string(), log_editor);

    app.handle_ui_click(crate::ui_system::UiId::LspLogArea(0));
    assert_eq!(app.ide_panel.lsp_logs_focused.as_deref(), Some("ruff"));
    assert!(app.is_dragging_lsp_log);
    assert_eq!(
        app.ide_panel
            .lsp_log_editors
            .get("ruff")
            .and_then(|ed| ed.selection_anchor),
        None
    );
}
