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
fn switch_to_highlighted_tab_reuses_cached_highlight_without_restart() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs.push(tab_with(
        "first.py",
        Some("/tmp/first.py"),
        "print('first')\n",
    ));
    app.tabs.push(tab_with(
        "second.py",
        Some("/tmp/second.py"),
        "def cached():\n    return 1\n",
    ));
    app.active_tab = 0;
    app.editor = editor_with("print('first')\n");
    app.editor.version = 11;
    app.file_path = Some(PathBuf::from("/tmp/first.py"));
    app.file_extension = "py".to_string();
    app.base_title = "first.py".to_string();
    app.is_highlighted_once = true;
    app.is_highlight_complete = true;
    app.highlighter.current_version = app.editor.version;

    app.tabs[1].editor.version = 22;
    app.tabs[1].editor.foldable_lines.insert(0, 1);
    app.tabs[1].editor.folded_lines.insert(0);
    app.tabs[1].editor.folded_start_bytes.insert(0);
    app.tabs[1].spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: 3,
        color: crate::highlighter::DRACULA_PINK,
    }];
    app.tabs[1].foldable_ranges = vec![(0, 26, true, false)];
    app.tabs[1].is_highlighted_once = true;
    app.tabs[1].is_highlight_complete = true;
    let target_version = app.tabs[1].editor.version;

    app.switch_to_tab(1);

    assert_eq!(app.editor.version, target_version);
    assert_eq!(app.highlighter.current_version, target_version);
    assert!(app.is_highlighted_once);
    assert_eq!(app.highlighter.spans.len(), 1);
    assert_eq!(app.highlighter.spans[0].start, 0);
    assert!(app.editor.folded_lines.contains(&0));
    assert!(app.editor.folded_start_bytes.contains(&0));
}

#[test]
fn switch_back_to_partial_large_tab_restarts_full_highlight_without_clearing_cache() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs.push(tab_with(
        "small.py",
        Some("/tmp/small.py"),
        "print('small')\n",
    ));
    app.tabs.push(tab_with(
        "large.py",
        Some("/tmp/large.py"),
        "def partial():\n    return 1\n",
    ));
    app.active_tab = 0;
    app.editor = editor_with("print('small')\n");
    app.editor.version = 7;
    app.file_path = Some(PathBuf::from("/tmp/small.py"));
    app.file_extension = "py".to_string();
    app.base_title = "small.py".to_string();
    app.is_highlighted_once = true;
    app.is_highlight_complete = true;

    app.tabs[1].editor.version = 42;
    app.tabs[1].spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: 3,
        color: crate::highlighter::DRACULA_PINK,
    }];
    app.tabs[1].is_highlighted_once = true;
    app.tabs[1].is_highlight_complete = false;

    app.switch_to_tab(1);

    assert_eq!(app.editor.version, 42);
    assert!(app.is_highlighted_once);
    assert!(!app.is_highlight_complete);
    assert!(!app.highlighter.is_complete);
    assert_eq!(app.highlighter.current_version, 42);
    assert_eq!(app.highlighter.spans.len(), 1);
    assert_eq!(
        app.highlighter.spans[0].color,
        crate::highlighter::DRACULA_PINK
    );
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
fn definition_jump_to_open_file_does_not_restart_highlighter() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs
        .push(tab_with("main.py", Some("/tmp/main.py"), "first\nsecond\n"));
    app.active_tab = 0;
    app.sync_active_tab();
    app.editor.version = 41;
    app.highlighter.current_version = 41;
    app.is_highlighted_once = true;

    app.jump_to_definition_target(DefinitionJumpTarget {
        path: PathBuf::from("/tmp/main.py"),
        line: 1,
        col: 0,
    });

    assert_eq!(app.editor.cursor, "first\n".len());
    assert_eq!(app.highlighter.current_version, 41);
    assert!(app.is_highlighted_once);
}

#[test]
fn lsp_position_jump_to_closed_large_file_prioritizes_target_region() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;

    let unique = format!(
        "rriter-closed-lsp-position-jump-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("large.rs");
    let prefix_line = "fn filler() {\n    let value = 1;\n}\n";
    let prefix = prefix_line.repeat(1400);
    let target = "fn target_jump() {\n    let value = 2;\n}\n";
    let suffix = prefix_line.repeat(1400);
    let text = format!("{prefix}{target}{suffix}");
    let target_line = (prefix.matches('\n').count()) as u32;
    let target_offset = prefix.len() + "fn ".len();
    assert!(text.len() > crate::highlighter::TREE_SITTER_HIGHLIGHT_MAX_BYTES);
    std::fs::write(&path, text).unwrap();

    let was_open = app.jump_to_lsp_position_in_file(path.clone(), target_line, 3, true, 0.42);

    assert!(!was_open);
    assert_eq!(app.editor.cursor, target_offset);
    assert_eq!(app.highlighter.current_version, app.editor.version);
    assert!(app.highlighter.spans.iter().any(|span| {
        span.start <= target_offset && span.end >= target_offset + "target_jump".len()
    }));

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn problem_jump_to_closed_large_file_prioritizes_target_region() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.show_welcome = false;

    let unique = format!(
        "rriter-problem-closed-jump-test-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("large.rs");
    let prefix_line = "fn filler() {\n    let value = 1;\n}\n";
    let prefix = prefix_line.repeat(1400);
    let target = "fn target_problem_jump() {\n    let value = 2;\n}\n";
    let suffix = prefix_line.repeat(1400);
    let text = format!("{prefix}{target}{suffix}");
    let target_line = (prefix.matches('\n').count()) as u32;
    let target_offset = prefix.len() + "fn ".len();
    assert!(text.len() > crate::highlighter::TREE_SITTER_HIGHLIGHT_MAX_BYTES);
    std::fs::write(&path, text).unwrap();

    let mut lsp = crate::lsp::LspManager::new(Vec::new());
    lsp.diagnostics.insert(
        path.clone(),
        vec![crate::lsp::Diagnostic {
            start_line: target_line,
            start_col: 0,
            end_line: target_line,
            end_col: 3,
            severity: crate::lsp::DiagSeverity::Error,
            code: None,
            code_href: None,
            message: std::sync::Arc::<str>::from("problem"),
            source: None,
            quickfixes: Vec::new().into_boxed_slice(),
            tags: Vec::new().into_boxed_slice(),
        }]
        .into(),
    );
    app.lsp = Some(lsp);
    app.ide_panel.flat_diags.push((path.clone(), 0));

    app.handle_ui_click(crate::ui_system::UiId::ProblemJump(0));

    assert_eq!(app.editor.cursor, target_offset);
    assert_eq!(app.highlighter.current_version, app.editor.version);
    assert!(app.highlighter.spans.iter().any(|span| {
        span.start <= target_offset && span.end >= target_offset + "target_problem_jump".len()
    }));

    std::fs::remove_file(path).ok();
    std::fs::remove_dir(dir).ok();
}

#[test]
fn problem_jump_to_open_file_does_not_restart_highlighter() {
    let Some(mut app) = test_app() else {
        return;
    };
    let path = PathBuf::from("/tmp/problem-open.py");
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.tabs.push(tab_with(
        "problem-open.py",
        Some("/tmp/problem-open.py"),
        "first\nsecond\n",
    ));
    app.active_tab = 0;
    app.sync_active_tab();
    app.editor.version = 77;
    app.highlighter.current_version = 77;
    app.is_highlighted_once = true;

    let mut lsp = crate::lsp::LspManager::new(Vec::new());
    lsp.diagnostics.insert(
        path.clone(),
        vec![crate::lsp::Diagnostic {
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
            severity: crate::lsp::DiagSeverity::Warning,
            code: None,
            code_href: None,
            message: std::sync::Arc::<str>::from("problem"),
            source: None,
            quickfixes: Vec::new().into_boxed_slice(),
            tags: Vec::new().into_boxed_slice(),
        }]
        .into(),
    );
    app.lsp = Some(lsp);
    app.ide_panel.flat_diags.push((path, 0));

    app.handle_ui_click(crate::ui_system::UiId::ProblemJump(0));

    assert_eq!(app.editor.cursor, "first\n".len());
    assert_eq!(app.highlighter.current_version, 77);
    assert!(app.is_highlighted_once);
}

#[test]
fn search_jump_moves_selection_without_restarting_highlighter() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("one\ntwo\none\n");
    app.editor.version = 42;
    app.highlighter.current_version = 42;
    app.is_highlighted_once = true;
    app.search_results = vec![(0, 3), (8, 11)];
    app.search_current_idx = Some(1);

    app.jump_to_search_result();

    assert_eq!(app.editor.cursor, 11);
    assert_eq!(app.editor.selection_anchor, Some(8));
    assert_eq!(app.highlighter.current_version, 42);
    assert!(app.is_highlighted_once);
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
