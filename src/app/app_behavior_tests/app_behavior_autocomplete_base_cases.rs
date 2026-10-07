use super::*;
use crate::platform::Clipboard;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn test_theme() -> crate::renderer::Theme {
    crate::renderer::Theme {
        bg: [0.156, 0.164, 0.211, 1.0],
        fg: [0.972, 0.972, 0.949, 1.0],
        sel: [0.55, 0.55, 0.55, 1.0],
        minimap_bg: [0.129, 0.133, 0.172, 1.0],
        line_num: [0.384, 0.447, 0.643, 1.0],
        minimap_cursor: [0.55, 0.55, 0.55, 1.0],
        modified_unsaved: [1.0, 0.474, 0.776, 1.0],
        modified_saved: [0.313, 0.980, 0.482, 1.0],
        diag_warn: [0.945, 0.980, 0.549, 1.0],
        diag_error: [1.0, 0.333, 0.333, 1.0],
        unused: [0.48, 0.48, 0.48, 0.6],
        ..crate::renderer::Theme::for_id(crate::theme::ThemeId::Dracula, [0.0; 4])
    }
}

pub(crate) fn editor_with(text: &str) -> Editor {
    let mut editor = Editor::new(text.len() + 64);
    let _ = editor.insert_str(text);
    editor.cursor = text.len();
    editor.clear_history();
    editor.set_original_text();
    editor.sync_edits.clear();
    editor
}

pub(crate) fn tab_with(title: &str, path: Option<&str>, text: &str) -> EditorTab {
    EditorTab {
        editor: editor_with(text),
        file_path: path.map(PathBuf::from),
        file_key: path.map(PathBuf::from).as_deref().map(crate::platform::PathKey::new),
        text_file_format: crate::platform::TextFileFormat {
            encoding: crate::platform::TextEncoding::Utf8,
            line_ending: crate::platform::LineEnding::Lf,
        },
        base_title: title.to_string(),
        file_extension: path
            .and_then(|p| std::path::Path::new(p).extension())
            .map(|ext| ext.to_string_lossy().to_string())
            .unwrap_or_default(),
        markdown: Default::default(),
        pdf: None,
        image: None,
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: false,
        is_highlight_complete: false,
        icon_key: "default_file",
        syntax_errors: Vec::new(),
        closing_hints: Default::default(),
        deleted: false,
        load: crate::app::TabLoad::Loaded,
        kind: EditorTabKind::Normal,
    }
}

pub(crate) fn test_app() -> Option<App> {
    let now = Instant::now();
    Some(App {
        keymap: crate::keymap::Keymap::build(&crate::keymap::KeymapOverrides::default()),
        keymap_overrides: crate::keymap::KeymapOverrides::default(),
        empty_ide_open_label: "Ctrl+O  — открыть файл".into(),
        project_search_run_label: "Literal-only. Ctrl+Enter или кнопка запуска.".into(),
        automation: None,
        database_runtime: None,
        scroll_render_bench: None,
        pending_key_log: None,
        gl_config: None,
        gl_context: None,
        gl_surface: None,
        window: None,
        confirm_dialog: crate::app::ConfirmDialog::default(),
        protected_saves: crate::app::ProtectedSaves::default(),
        external_requests: crate::platform::ExternalRequestLog::default(),
        settings_scroll: crate::scroll::ScrollState::new(15.0),
        settings_general_scroll: crate::scroll::ScrollState::new(7.0),
        settings_database_scroll: crate::scroll::ScrollState::new(7.0),
        settings_general_max_scroll: 0.0,
        settings_database_max_scroll: 0.0,
        settings_appearance_scroll: crate::scroll::ScrollState::new(7.0),
        settings_appearance_max_scroll: 0.0,
        keymap_settings: crate::app::keymap_settings::KeymapSettingsState::default(),
        tab_scroll: crate::scroll::ScrollState::new(15.0),
        renderer: None,
        editor: Editor::new(128),
        clipboard: Some(Clipboard::deferred_system()),
        theme: test_theme(),
        system_selection: [0.0; 4],
        editor_theme_id: crate::theme::ThemeId::Dracula,
        ui_theme_id: crate::theme::ThemeId::Dracula,
        theme_linked: true,
        base_title: "Безымянный".to_string(),
        file_path: None,
        file_key: None,
        text_file_format: crate::platform::TextFileFormat {
            encoding: crate::platform::TextEncoding::Utf8,
            line_ending: crate::platform::LineEnding::Lf,
        },
        file_extension: String::new(),
        markdown: Default::default(),
        markdown_toc: Default::default(),
        markdown_media: crate::markdown_media::MarkdownMedia::with_loader(std::sync::Arc::new(|_| {
            Err(crate::markdown_media::MediaError::Unsupported)
        })),
        highlighter: crate::highlighter::Highlighter::new(),
        closing_hint_state: Default::default(),
        closing_hint_settings: Default::default(),
        last_sent_version: u64::MAX,
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        last_frame: now,
        last_action: now,
        last_blink_state: true,
            modifiers: winit::keyboard::ModifiersState::empty(),
            left_shift_down: false,
        ctrl_wheel_multiplier: crate::CTRL_WHEEL_MULTIPLIER_DEFAULT,
        git_blame_inline: false,
        git_blame_delay_ms: crate::state_persistence::GIT_BLAME_DELAY_DEFAULT,
        is_dragging: false,
        is_editor_drag_pending: false,
        is_focused: true,
        render_suspended: false,
        current_cursor: winit::window::CursorIcon::Default,
        show_fps: false,
        window_width: 1000.0,
        window_height: 800.0,
        last_resize_time: None,
        last_click_time: now,
        click_count: 0,
        last_click_pos: (0.0, 0.0),
        last_click_ui_id: None,
        open_file_rx: None,
        save_file_rx: None,
        api_import_file_rx: None,
        api_body_file_rx: None,
        api_openapi_export_rx: None,
        api_load_rx: Vec::new(),
        api_request_rx: Vec::new(),
        api_mock_ty_rx: None,
        show_welcome: true,
        recent_files: Vec::new(),
        is_ide_mode: false,
        ide_workspaces: Vec::new(),
        ide_ignore_patterns: Vec::new(),
        settings_ignore_editor: Editor::new(128),
        settings_ignore_focused: false,
        settings_ignore_scroll_x: 0.0,
        is_dragging_settings_ignore: false,
        open_folder_rx: None,
        tool_paths: crate::platform::ToolPaths::default(),
        dart_settings: crate::app::DartSettings::default(),
        rust_settings: crate::app::RustSettings::default(),
        settings_tool_picker_rx: None,
        tool_installer: crate::app::tool_installer::ToolInstaller::default(),
        dart_tool_state: crate::app::tool_installer::DartToolState::default(),
        show_search: false,
        search_anim_y: -120.0,
        search_editor: Editor::new(256),
        search_focused: false,
        search_case_sensitive: false,
        search_results: Vec::new(),
        search_current_idx: None,
        is_dragging_search: false,
        is_dragging_lsp_log: false,
        faq_editor: Editor::new(128),
        is_ready: false,
        is_highlighted_once: false,
        is_highlight_complete: false,
        should_maximize: false,
        autocomplete_active: false,
        autocomplete_options: Vec::new(),
        autocomplete_selected_idx: 0,
        autocomplete_anim_progress: 0.0,
        autocomplete_scroll: crate::scroll::ScrollState::new(15.0),
        autocomplete_hovered_idx: None,
        autocomplete_rect: None,
        autocomplete_anchor: None,
        autocomplete_mode: AutocompleteMode::TreeSitter,
        autocomplete_pending_request_id: None,
        autocomplete_pending_request_mode: None,
        autocomplete_pending_request_path: None,
        autocomplete_pending_context_key: None,
        autocomplete_signature_request_id: None,
        autocomplete_signature_items: Vec::new(),
        autocomplete_detail_request_id: None,
        autocomplete_detail_word: None,
        autocomplete_detail_request_path: None,
        autocomplete_detail_context_key: None,
        autocomplete_detail_popup: None,
        autocomplete_detail_rect: None,
        autocomplete_detail_placement: None,
        autocomplete_detail_max_scroll: 0.0,
        autocomplete_min_width: 0.0,
        autocomplete_detail_min_width: 0.0,
        autocomplete_detail_min_height: 0.0,
        autocomplete_detail_selection_anchor: None,
        autocomplete_detail_selection_cursor: None,
        autocomplete_detail_selecting: false,
        autocomplete_apply_pending_response: false,
        autocomplete_cache: None,
        autocomplete_detail_cache: None,
        current_sticky_lines: Vec::new(),
        target_sticky_lines: Vec::new(),
        sticky_anim_progress: 1.0,
        sticky_anim_is_adding: false,
        show_settings: false,
        settings_anim_progress: 0.0,
        settings_y: 10000.0,
        settings_tab: 0,
        settings_ide_scroll: crate::scroll::ScrollState::new(7.0),
        ide_panel: IdePanelState::default(),
        file_tree_rx: None,
        file_tree_notify_rx: None,
        file_tree_watcher_stop_tx: None,
        file_tree_watched_dirs: Vec::new(),
        external_changes_rx: None,
        external_changes_pending: false,
        git_diff_rx: Vec::new(),
        git_blame_rx: Vec::new(),
        git_blame_message_rx: Vec::new(),
        inline_git_diff_rx: None,
        inline_git_popup: None,
        inline_blame_dwell: crate::app::git_blame::InlineBlameDwell::default(),
        readonly_notice_until: None,
        readonly_notice_text: "Файл открыт в режиме только чтение".to_string(),
        lsp: None,
        lsp_actions_menu: None,
        pending_fix_all_id: None,
        ctrl_definition: CtrlDefinitionState::default(),
        python_inlay_hints: Vec::new(),
        python_inlay_hint_path: None,
        python_inlay_hint_range: None,
        python_inlay_hint_version: 0,
        python_inlay_hint_pending_request_id: None,
        python_inlay_hint_pending_path: None,
        python_inlay_hint_pending_range: None,
        python_inlay_hint_pending_version: 0,
        python_inlay_hint_cache: rustc_hash::FxHashMap::default(),
        ui_registry: crate::ui_system::UiRegistry::new(),
        hover: crate::app::mouse::HoverState::default(),
        tabs: Vec::new(),
        active_tab: 0,
        pdf_engine: crate::app::pdf_tab::PdfEngineState::NotStarted,
        pdf_library_source: crate::app::pdf_tab::PdfLibrarySource::ProcessEnv,
        pdf_worker: None,
        next_doc_id: 1,
        pdf_textures_to_free: Vec::new(),
        pdf_dark_pages: true,
        run_ide_on_startup: false,
        headless_mode: false,
        ui_waker: crate::ui_waker::UiWaker::counting(),
        startup_trace: crate::startup_trace::StartupTrace::disabled(),
        startup_deferred_pending: false,
        ide_preload: None,
        ide_deferred: crate::app::IdeDeferred::None,
        startup_editor_pending: None,
        startup_editor_reveal_at: None,
    })
}

fn seed_inlay_hint_state(
    app: &mut App,
    path: &str,
    extension: &str,
    hints: Vec<PythonInlayHint>,
) {
    let path = PathBuf::from(path);
    let range = (0, app.editor.line_offsets.len() as u32);
    let version = app.editor.version;
    app.file_path = Some(path.clone());
    app.file_extension = extension.to_string();
    app.python_inlay_hints = hints;
    app.python_inlay_hint_path = Some(path.clone());
    app.python_inlay_hint_range = Some(range);
    app.python_inlay_hint_version = version;
    app.python_inlay_hint_cache.insert(
        (path, extension.to_string()),
        (version, range, app.python_inlay_hints.clone()),
    );
}

#[test]
fn local_inlay_hint_shift_keeps_dart_hint_current_and_invalidates_cached_range() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("call(value)\n");
    seed_inlay_hint_state(
        &mut app,
        "inlay.dart",
        "dart",
        vec![PythonInlayHint {
            byte_offset: 5,
            label: Arc::<str>::from("value: "),
        }],
    );
    let old_version = app.editor.version;

    let _ = app.editor.replace_range(0, 0, "x");
    assert_ne!(app.editor.version, old_version);
    let edits = std::mem::take(&mut app.editor.sync_edits);
    app.shift_current_python_inlay_hints_for_edits(&edits);

    assert_eq!(app.python_inlay_hints[0].byte_offset, 6);
    assert_eq!(app.python_inlay_hint_version, app.editor.version);
    assert!(
        app.python_inlay_hint_path.as_ref() == app.file_path.as_ref()
            && app.python_inlay_hint_version == app.editor.version
    );
    assert_eq!(app.python_inlay_hint_range, None);
    assert!(app.python_inlay_hint_cache.is_empty());
}

#[test]
fn local_inlay_hint_delete_shifts_survivor_and_removes_intersected_hint() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("abcdefghij\n");
    seed_inlay_hint_state(
        &mut app,
        "inlay.py",
        "py",
        vec![
            PythonInlayHint {
                byte_offset: 2,
                label: Arc::<str>::from("removed: "),
            },
            PythonInlayHint {
                byte_offset: 8,
                label: Arc::<str>::from("kept: "),
            },
        ],
    );

    let _ = app.editor.replace_range(1, 4, "");
    let edits = std::mem::take(&mut app.editor.sync_edits);
    app.shift_current_python_inlay_hints_for_edits(&edits);

    assert_eq!(
        app.python_inlay_hints,
        vec![PythonInlayHint {
            byte_offset: 5,
            label: Arc::<str>::from("kept: "),
        }]
    );
    assert_eq!(app.python_inlay_hint_version, app.editor.version);
    assert_eq!(app.python_inlay_hint_range, None);
    assert!(app.python_inlay_hint_cache.is_empty());
}

fn completion(
    word: &str,
    kind: SymbolKind,
    scope_start: usize,
    scope_end: usize,
) -> CompletionItem {
    CompletionItem {
        word: word.to_string(),
        kind,
        scope_start,
        scope_end,
    }
}

#[test]
fn search_update_finds_nearest_match_preserves_previous_and_honors_case() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("alpha beta\nAlpha beta\nbeta tail");
    app.editor.cursor = 18;
    app.search_editor = editor_with("beta");
    app.update_search();
    assert_eq!(app.search_results.len(), 3);
    assert_eq!(app.search_current_idx, Some(1));
    let previous = app.search_current_idx;
    app.update_search();
    assert_eq!(app.search_current_idx, previous);

    app.search_case_sensitive = true;
    app.search_editor = editor_with("Alpha");
    app.update_search();
    assert_eq!(app.search_results, vec![(11, 16)]);
    assert_eq!(app.search_current_idx, Some(0));

    app.search_editor = Editor::new(32);
    app.update_search();
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_current_idx, None);
}

#[test]
fn autocomplete_filters_scores_scrolls_and_applies_selected_completion() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("pri");
    app.editor.cursor = 3;
    app.highlighter.completions = vec![
        completion("print", SymbolKind::Function, 0, 100),
        completion("private_value", SymbolKind::Variable, 1, 100),
        completion("printf", SymbolKind::Function, 10, 20),
        completion("pri", SymbolKind::Variable, 0, 100),
    ];

    app.update_autocomplete();
    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_selected_idx, 0);
    assert_eq!(app.autocomplete_options.len(), 2);
    assert_eq!(app.autocomplete_options[0].0.word, "private_value");
    assert_eq!(app.autocomplete_options[1].0.word, "print");

    app.autocomplete_selected_idx = 1;
    app.autocomplete_scroll.target = 200.0;
    app.ensure_autocomplete_visible();
    assert!(app.autocomplete_scroll.target <= 36.0);

    app.apply_autocomplete();
    assert_eq!(app.editor.get_full_text(), "print");
    assert!(!app.autocomplete_active);
    assert_eq!(app.autocomplete_selected_idx, 0);
    assert_eq!(app.autocomplete_scroll.target, 0.0);
}

fn sql_completion_test_app(
    text: &str,
    metadata: crate::app::database::DatabaseQueryCompletionMetadata,
) -> Option<App> {
    let mut app = test_app()?;
    app.editor = editor_with(text);
    app.file_extension = "sql".to_string();
    let mut tab = tab_with("SQL Console", None, text);
    tab.file_extension = "sql".to_string();
    tab.kind = EditorTabKind::DatabaseQuery(
        crate::app::database::DatabaseQueryTabMeta {
            console_id: crate::app::database::SqlConsoleId(7),
            connection_id: crate::app::database::DatabaseConnectionId(3),
            database_name: "postgres".to_string(),
            title: "SQL Console".to_string(),
        },
        crate::app::database::DatabaseQueryTabState {
            completion: metadata,
            completion_loaded: true,
            ..crate::app::database::DatabaseQueryTabState::default()
        },
    );
    app.tabs = vec![tab];
    app.active_tab = 0;
    Some(app)
}
fn sql_completion_metadata() -> crate::app::database::DatabaseQueryCompletionMetadata {
    crate::app::database::DatabaseQueryCompletionMetadata {
        columns: vec![crate::app::database::DatabaseQueryCompletionColumn {
            table_name: "items".to_string(),
            column_name: "frame".to_string(),
            data_type: "text".to_string(),
        }],
        functions: vec!["format".to_string()],
        ..crate::app::database::DatabaseQueryCompletionMetadata::default()
    }
}
#[test]
fn editor_and_sql_completion_share_non_restarting_popup_session_updates() {
    let Some(mut editor_app) = test_app() else {
        return;
    };
    editor_app.editor = editor_with("pr");
    editor_app.highlighter.completions = vec![
        completion("print", SymbolKind::Function, 0, 100),
        completion("private", SymbolKind::Variable, 0, 100),
    ];
    editor_app.update_autocomplete();
    editor_app.autocomplete_anim_progress = 0.63;
    editor_app.autocomplete_scroll.current = 18.0;
    editor_app.autocomplete_scroll.target = 18.0;
    editor_app.autocomplete_selected_idx = editor_app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "print")
        .expect("print completion");
    editor_app.editor = editor_with("pri");
    editor_app.update_autocomplete();
    assert_eq!(editor_app.autocomplete_anim_progress, 0.63);
    assert_eq!(editor_app.autocomplete_scroll.target, 18.0);
    assert_eq!(
        editor_app.autocomplete_options[editor_app.autocomplete_selected_idx]
            .0
            .word,
        "print"
    );
    let Some(mut sql_app) = sql_completion_test_app("SELECT *\nF", sql_completion_metadata())
    else {
        return;
    };
    sql_app.update_active_database_query_completion(false);
    let from_index = sql_app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "FROM")
        .expect("FROM completion");
    sql_app.autocomplete_selected_idx = from_index;
    sql_app.autocomplete_anim_progress = 0.63;
    sql_app.autocomplete_scroll.current = 18.0;
    sql_app.autocomplete_scroll.target = 18.0;
    let session_key = sql_app.autocomplete_pending_context_key.clone();
    sql_app.editor = editor_with("SELECT *\nFR");
    sql_app.update_active_database_query_completion(false);
    assert_eq!(sql_app.autocomplete_anim_progress, 0.63);
    assert_eq!(sql_app.autocomplete_pending_context_key, session_key);
    assert_eq!(
        sql_app.autocomplete_options[sql_app.autocomplete_selected_idx]
            .0
            .word,
        "FROM"
    );
    sql_app.editor = editor_with("SELECT *\nF");
    sql_app.update_active_database_query_completion(false);
    assert_eq!(sql_app.autocomplete_anim_progress, 0.63);
    assert_eq!(sql_app.autocomplete_pending_context_key, session_key);
}
#[test]
fn sql_completion_schema_refresh_preserves_scroll_for_surviving_selection() {
    let mut metadata = sql_completion_metadata();
    metadata.columns.extend((0..16).map(|index| {
        crate::app::database::DatabaseQueryCompletionColumn {
            table_name: "items".to_string(),
            column_name: format!("fr_item_{index:02}"),
            data_type: "text".to_string(),
        }
    }));
    let Some(mut app) = sql_completion_test_app("SELECT *\nFR", metadata) else {
        return;
    };
    app.update_active_database_query_completion(false);
    app.autocomplete_selected_idx = app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "fr_item_10")
        .expect("stable SQL completion item");
    app.autocomplete_anim_progress = 0.74;
    app.autocomplete_scroll.current = 180.0;
    app.autocomplete_scroll.target = 180.0;
    if let EditorTabKind::DatabaseQuery(_, state) = &mut app.tabs[0].kind {
        state.completion.columns.push(
            crate::app::database::DatabaseQueryCompletionColumn {
                table_name: "items".to_string(),
                column_name: "fr_item_99".to_string(),
                data_type: "text".to_string(),
            },
        );
    }
    app.update_active_database_query_completion(false);
    assert_eq!(app.autocomplete_anim_progress, 0.74);
    assert_eq!(app.autocomplete_scroll.current, 180.0);
    assert_eq!(app.autocomplete_scroll.target, 180.0);
    assert_eq!(
        app.autocomplete_options[app.autocomplete_selected_idx].0.word,
        "fr_item_10"
    );
    if let EditorTabKind::DatabaseQuery(_, state) = &mut app.tabs[0].kind {
        state.completion.columns.retain(|column| column.column_name == "fr_item_10");
        state.completion.functions.clear();
    }
    app.update_active_database_query_completion(false);
    let max_scroll = ((app.autocomplete_options.len() as f32 - 7.0).max(0.0)) * 36.0;
    assert!(app.autocomplete_scroll.target <= max_scroll);
    assert!(app.autocomplete_scroll.current <= max_scroll);
}
#[test]
fn sql_completion_enter_tab_and_escape_use_shared_popup_actions() {
    for key in [
        winit::keyboard::KeyCode::Enter,
        winit::keyboard::KeyCode::Tab,
    ] {
        let Some(mut app) = sql_completion_test_app("SELECT *\nFR", sql_completion_metadata())
        else {
            return;
        };
        app.update_active_database_query_completion(false);
        app.autocomplete_selected_idx = app
            .autocomplete_options
            .iter()
            .position(|(item, _)| item.word == "FROM")
            .expect("FROM completion");
        assert_eq!(
            app.handle_active_autocomplete_key(
                winit::keyboard::PhysicalKey::Code(key),
                false,
            ),
            AutocompletePopupKeyResult::Consumed
        );
        assert_eq!(app.editor.get_full_text(), "SELECT *\nFROM");
        assert_eq!(app.editor.cursor, "SELECT *\nFROM".len());
        assert!(!app.autocomplete_active);
    }
    let Some(mut app) = sql_completion_test_app("SELECT *\nFR", sql_completion_metadata()) else {
        return;
    };
    app.update_active_database_query_completion(false);
    assert_eq!(
        app.handle_active_autocomplete_key(
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape),
            false,
        ),
        AutocompletePopupKeyResult::Consumed
    );
    assert_eq!(app.editor.get_full_text(), "SELECT *\nFR");
    assert!(!app.autocomplete_active);
}
#[test]
fn sql_completion_list_refresh_preserves_animation_selection_and_apply_range() {
    let Some(mut app) = sql_completion_test_app("SELECT *\nFR", sql_completion_metadata()) else {
        return;
    };
    app.update_active_database_query_completion(false);
    app.autocomplete_selected_idx = app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "FROM")
        .expect("FROM completion");
    app.autocomplete_anim_progress = 0.81;
    if let EditorTabKind::DatabaseQuery(_, state) = &mut app.tabs[0].kind {
        state.completion.functions.push("frame_fn".to_string());
    }
    app.update_active_database_query_completion(false);
    assert_eq!(app.autocomplete_anim_progress, 0.81);
    assert_eq!(
        app.autocomplete_options[app.autocomplete_selected_idx].0.word,
        "FROM"
    );
    app.apply_autocomplete();
    assert_eq!(app.editor.get_full_text(), "SELECT *\nFROM");
    assert_eq!(app.editor.cursor, "SELECT *\nFROM".len());
    assert!(!app.autocomplete_active);
}
#[test]
fn applying_autocomplete_recovers_from_a_stale_selected_index() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("pri");
    app.editor.cursor = 3;
    app.highlighter.completions = vec![completion("print", SymbolKind::Function, 0, 100)];
    app.update_autocomplete();
    assert_eq!(app.autocomplete_options.len(), 1);

    app.autocomplete_selected_idx = usize::MAX;
    app.apply_autocomplete();

    assert_eq!(app.editor.get_full_text(), "print");
    assert!(!app.autocomplete_active);
}
#[test]
fn ty_import_autocomplete_waits_for_prefix_and_requires_module() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("");
    app.autocomplete_mode = AutocompleteMode::TyImports;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "Path".to_string(),
        kind: SymbolKind::Class,
        module: Some("pathlib".to_string()),
        detail: Some("type[Path]".to_string()),
        insert_text: Some("Path".to_string()),
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);
    assert!(app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());

    app.editor = editor_with("Pa");
    app.autocomplete_mode = AutocompleteMode::TyImports;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "Path".to_string(),
            kind: SymbolKind::Class,
            module: Some("pathlib".to_string()),
            detail: Some("type[Path]".to_string()),
            insert_text: Some("Path".to_string()),
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "ParamSpec".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("typing special form".to_string()),
            insert_text: Some("ParamSpec".to_string()),
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);
    assert_eq!(app.autocomplete_options.len(), 1);
    assert_eq!(app.autocomplete_options[0].0.word, "Path");
    assert_eq!(
        app.autocomplete_options[0].0.module.as_deref(),
        Some("pathlib")
    );
}

#[test]
fn ty_import_autocomplete_promotes_unknown_items_with_module() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("pa");
    app.autocomplete_mode = AutocompleteMode::TyImports;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "pathlib".to_string(),
        kind: SymbolKind::Unknown,
        module: Some("stdlib".to_string()),
        detail: None,
        insert_text: Some("pathlib".to_string()),
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    assert_eq!(app.autocomplete_options.len(), 1);
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Module);
}

#[test]
fn ty_import_completion_inserts_word_and_appends_import_without_cursor_jump() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("import os\nimport sys\n\nPa");
    app.file_path = Some(PathBuf::from("main.py"));
    app.file_extension = "py".to_string();
    app.autocomplete_mode = AutocompleteMode::TyImports;

    let item = AutocompleteItem {
        word: "Path".to_string(),
        kind: SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("pathlib".to_string()),
        module_path: Some("pathlib".to_string()),
        detail: None,
        insert_text: Some("Path".to_string()),
        text_edit: None,
        additional_text_edits: vec![crate::lsp::TextChange {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            new_text: "from pathlib import Path\n".to_string(),
        }],
    };

    app.apply_lsp_completion_item(&item);

    assert_eq!(
        app.editor.get_full_text(),
        "import os\nimport sys\nfrom pathlib import Path\n\nPath"
    );
    assert_eq!(app.editor.cursor, app.editor.len());
}

#[test]
fn ty_import_completion_with_text_edit_appends_after_multiline_import_region() {
    let Some(mut app) = test_app() else {
        return;
    };
    let text = concat!(
        "import datetime as dt\n",
        "from decimal import Decimal\n",
        "\n",
        "from car_wash.core.service import (\n",
        "    GenericCRUDService,\n",
        ")\n",
        "from car_wash.utils.schemas.types import (\n",
        "    StateEnum,\n",
        ")\n",
        "\n",
        "class BookingService:\n",
        "    Repo",
    );
    app.editor = editor_with(text);
    app.file_path = Some(PathBuf::from("main.py"));
    app.file_extension = "py".to_string();
    app.autocomplete_mode = AutocompleteMode::TyImports;

    let item = AutocompleteItem {
        word: "RepoBase".to_string(),
        kind: SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("car_wash.core.db.repo_base".to_string()),
        module_path: Some("car_wash.core.db.repo_base".to_string()),
        detail: None,
        insert_text: None,
        text_edit: Some(crate::lsp::TextChange {
            start_line: 11,
            start_col: 4,
            end_line: 11,
            end_col: 8,
            new_text: "RepoBase".to_string(),
        }),
        additional_text_edits: vec![crate::lsp::TextChange {
            start_line: 3,
            start_col: 0,
            end_line: 3,
            end_col: 0,
            new_text: "from car_wash.core.db.repo_base import RepoBase\n".to_string(),
        }],
    };

    app.apply_lsp_completion_item(&item);

    assert_eq!(
        app.editor.get_full_text(),
        concat!(
            "import datetime as dt\n",
            "from decimal import Decimal\n",
            "\n",
            "from car_wash.core.service import (\n",
            "    GenericCRUDService,\n",
            ")\n",
            "from car_wash.utils.schemas.types import (\n",
            "    StateEnum,\n",
            ")\n",
            "from car_wash.core.db.repo_base import RepoBase\n",
            "\n",
            "class BookingService:\n",
            "    RepoBase",
        )
    );
    assert_eq!(app.editor.cursor, app.editor.len());
}

include!("app_behavior_ty_detail_tests.rs");
